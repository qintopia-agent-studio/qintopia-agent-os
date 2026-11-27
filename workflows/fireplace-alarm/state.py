"""Durable alarm state for the single fireplace detector and its two recipients."""

import sqlite3
import time
from contextlib import contextmanager
from pathlib import Path


DATABASE = Path("/var/lib/qintopia-tuya-alerts/state.db")
SIGNALS = {"smoke_sensor_state": 1, "co_state": 2}
PROFILES = ("silaoshi", "erhua")
INTERVAL = 30_000
LATE = 60_000
RETRY_DELAYS = (5_000, 25_000, 30_000, 60_000)


def now_ms():
    return time.time_ns() // 1_000_000


def connect(path=DATABASE, *, create=False):
    mode = "rwc" if create else "rw"
    db = sqlite3.connect(f"{Path(path).resolve().as_uri()}?mode={mode}", uri=True,
                         isolation_level=None, timeout=10)
    db.row_factory = sqlite3.Row
    db.execute("PRAGMA foreign_keys = ON")
    return db


@contextmanager
def transaction(db, *, write=True):
    db.execute("BEGIN IMMEDIATE" if write else "BEGIN")
    try:
        yield
        db.commit()
    except BaseException:
        db.rollback()
        raise


def initialize(db, armed_at):
    with transaction(db):
        db.execute("""CREATE TABLE IF NOT EXISTS installation (
            id INTEGER PRIMARY KEY CHECK (id=1), armed_at INTEGER NOT NULL)""")
        db.execute("""CREATE TABLE IF NOT EXISTS signals (
            code TEXT PRIMARY KEY, value TEXT NOT NULL, reported_at INTEGER NOT NULL)""")
        db.execute("""CREATE TABLE IF NOT EXISTS incidents (
            id INTEGER PRIMARY KEY, active INTEGER NOT NULL, hazards INTEGER NOT NULL,
            reported_at INTEGER NOT NULL, closed_at INTEGER)""")
        if "closed_at" not in {col["name"] for col in db.execute("PRAGMA table_info(incidents)")}:
            db.execute("ALTER TABLE incidents ADD COLUMN closed_at INTEGER")
            db.execute("UPDATE incidents SET closed_at=reported_at WHERE active=0")
        db.execute("CREATE UNIQUE INDEX IF NOT EXISTS one_unclosed ON incidents((1)) WHERE closed_at IS NULL")
        db.execute("""CREATE TABLE IF NOT EXISTS routes (
            incident INTEGER REFERENCES incidents(id), profile TEXT NOT NULL, due INTEGER,
            PRIMARY KEY(incident,profile))""")
        db.execute("""CREATE TABLE IF NOT EXISTS deliveries (
            incident INTEGER, profile TEXT, ordinal INTEGER, kind TEXT NOT NULL,
            hazards INTEGER NOT NULL, reported_at INTEGER NOT NULL, started_at INTEGER NOT NULL,
            status TEXT NOT NULL, receipt TEXT, attempt INTEGER NOT NULL DEFAULT 1, retry_at INTEGER,
            PRIMARY KEY(incident,profile,ordinal),
            FOREIGN KEY(incident,profile) REFERENCES routes(incident,profile))""")
        if "attempt" not in {col["name"] for col in db.execute("PRAGMA table_info(deliveries)")}:
            # Old failed/uncertain sends have no proof of a pre-launch failure. Never replay them.
            db.execute("ALTER TABLE deliveries ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1")
            db.execute("ALTER TABLE deliveries ADD COLUMN retry_at INTEGER")
        db.execute("""CREATE INDEX IF NOT EXISTS notified ON deliveries(profile,incident DESC)
                      WHERE kind='alarm' AND status='accepted'""")
        db.execute("""CREATE INDEX IF NOT EXISTS in_flight ON deliveries(profile,started_at)
                      WHERE status='reserved'""")
        db.execute("""CREATE TABLE IF NOT EXISTS shadow_alarms (
                      code TEXT PRIMARY KEY, reported_at INTEGER NOT NULL)""")
        db.execute("INSERT OR IGNORE INTO installation VALUES(1,?)", (armed_at,))


def _valid_points(points):
    return [p for p in points if p.get("code") in SIGNALS and p.get("value") in ("alarm", "normal")
            and type(p.get("time")) is int]


def _record(db, points, received, *, from_shadow):
    cutoff = db.execute("SELECT armed_at FROM installation WHERE id=1").fetchone()[0]
    resolved_at = 0
    changed = False
    for point in points:
        code, value, at = point["code"], point["value"], point["time"]
        if at < cutoff:
            continue
        if from_shadow and value == "alarm":
            # A shadow alarm can veto an all-clear, but only a push can open or escalate an incident.
            db.execute("""
                INSERT INTO shadow_alarms SELECT ?,? WHERE NOT EXISTS (
                    SELECT 1 FROM signals WHERE code=? AND
                      (reported_at>? OR (reported_at=? AND value='alarm')))
                ON CONFLICT(code) DO UPDATE SET reported_at=MAX(reported_at,excluded.reported_at)
            """, (code, at, code, at, at))
            continue
        if not from_shadow:
            cleared = db.execute("""DELETE FROM shadow_alarms WHERE code=? AND
                (reported_at<? OR (reported_at=? AND ?='alarm'))""", (code, at, at, value))
            if cleared.rowcount:
                resolved_at = max(resolved_at, at)
        change = db.execute("""
            INSERT INTO signals VALUES(?,?,?) ON CONFLICT(code) DO UPDATE
            SET value=excluded.value, reported_at=excluded.reported_at
            WHERE excluded.reported_at > signals.reported_at OR
              (excluded.reported_at=signals.reported_at AND excluded.value='alarm' AND signals.value='normal')
        """, (code, value, at))
        changed |= bool(change.rowcount)
    if not changed and not resolved_at:
        return
    signals = db.execute("SELECT * FROM signals").fetchall()
    active = sum(SIGNALS[p["code"]] for p in signals if p["value"] == "alarm")
    incident = db.execute("SELECT * FROM incidents WHERE closed_at IS NULL").fetchone()
    if incident is None:
        if resolved_at and not active:
            db.execute("""UPDATE incidents SET closed_at=MAX(closed_at,?)
                          WHERE id=(SELECT MAX(id) FROM incidents)""", (resolved_at,))
        if not active or from_shadow:
            return
    hazards = active | (incident["hazards"] if incident else 0)
    reported = max(p["reported_at"] for p in signals if SIGNALS[p["code"]] & (active or hazards))
    if incident is None:
        number = db.execute("INSERT INTO incidents(active,hazards,reported_at) VALUES(?,?,?)",
                            (active, hazards, reported)).lastrowid
        db.executemany("INSERT INTO routes(incident,profile,due) VALUES(?,?,?)",
                       [(number, role, received) for role in PROFILES])
        return
    closed = None
    if not active and db.execute("SELECT 1 FROM shadow_alarms LIMIT 1").fetchone() is None:
        closed = max(reported, resolved_at)
    db.execute("UPDATE incidents SET active=?,hazards=?,reported_at=?,closed_at=? WHERE id=?",
               (active, hazards, reported, closed, incident["id"]))


def apply(db, points, received):
    valid = _valid_points(points)
    if valid:
        with transaction(db):
            _record(db, valid, received, from_shadow=False)


def _alarm_job(db, incident, profile, now):
    key = (incident["id"], profile)
    past = db.execute("""SELECT * FROM deliveries WHERE incident=? AND profile=? AND kind='alarm'
                         ORDER BY ordinal""", key).fetchall()
    attempt = 1
    if past and past[-1]["status"] == "failed" and past[-1]["retry_at"] is not None:
        failed = past.pop()
        if now < failed["retry_at"]:
            return None
        attempt = failed["attempt"] + 1
    ordinal = len(past) + 1
    if ordinal > 3:
        return None
    active = incident["active"]
    # Earlier deadlines for new hazards are derived at claim time. A withdrawn escalation
    # cannot overwrite the ordinary 30/60-second schedule for this group.
    due = incident["initial_due"] if incident["initial_due"] is not None else incident["reported_at"]
    gained = not past or bool(active & ~past[-1]["hazards"])
    if past:
        due = max(past[0]["started_at"] + len(past) * INTERVAL, past[-1]["started_at"] + INTERVAL)
        if not gained and now < due:
            return None
    late = now - due > LATE or (gained and now - incident["reported_at"] > LATE)
    return dict(incident=key[0], latest_incident=key[0], profile=profile, ordinal=ordinal,
                kind="alarm", hazards=active, reported_at=incident["reported_at"],
                started_at=now, attempt=attempt, late=late)


def _recovery_job(db, profile, now):
    # A group's last accepted warning is its outstanding obligation. A new incident
    # supersedes it only once that group has actually received a newer warning.
    last = db.execute("""SELECT incident FROM deliveries WHERE profile=? AND kind='alarm'
                         AND status='accepted' ORDER BY incident DESC LIMIT 1""", (profile,)).fetchone()
    if last is None:
        return None
    number = last["incident"]
    previous = db.execute("""SELECT status,attempt,retry_at FROM deliveries
                             WHERE incident=? AND profile=? AND ordinal=4""", (number, profile)).fetchone()
    attempt = 1
    if previous:
        if previous["status"] != "failed" or previous["retry_at"] is None or now < previous["retry_at"]:
            return None
        attempt = previous["attempt"] + 1
    # A brief subsequent incident may have cleared before its first notice reached this group.
    closed = db.execute("""SELECT MAX(id) AS latest, MAX(hazards & 1) | MAX(hazards & 2) AS hazards,
                                  MAX(closed_at) AS reported_at FROM incidents WHERE id>=?""",
                        (number,)).fetchone()
    return dict(incident=number, latest_incident=closed["latest"], profile=profile, ordinal=4, kind="recovery",
                hazards=closed["hazards"], reported_at=closed["reported_at"], started_at=now,
                attempt=attempt, late=now - closed["reported_at"] > LATE)


def _pending(db, profile, now):
    # The Hermes call times out in 24 s. A crashed sender still consumes its reserved
    # slot; waiting one interval lets subsequent slots progress without overlapping a live send.
    busy = db.execute("""SELECT 1 FROM deliveries WHERE profile=? AND status='reserved'
                         AND started_at>? LIMIT 1""", (profile, now - INTERVAL)).fetchone()
    if busy:
        return None
    opened = db.execute("""SELECT i.*,r.due AS initial_due FROM incidents AS i JOIN routes AS r ON r.incident=i.id
                           WHERE i.closed_at IS NULL AND r.profile=?""", (profile,)).fetchone()
    if opened:
        return _alarm_job(db, opened, profile, now) if opened["active"] else None
    if db.execute("SELECT 1 FROM shadow_alarms LIMIT 1").fetchone():
        return None
    return _recovery_job(db, profile, now)


def claim(db, profile, now=None, verify_late=None):
    # Read a consistent plan, then release the database before making a cloud query.
    # Reconcile and reserve against current state in one write transaction.
    with transaction(db, write=False):
        candidate = _pending(db, profile, now_ms() if now is None else now)
    if candidate is None:
        return None
    snapshot = verify_late() if candidate["late"] and verify_late else []
    with transaction(db):
        if now is None:
            now = now_ms()
        if snapshot:
            latest = db.execute("SELECT MAX(id) FROM incidents").fetchone()[0]
            if latest != candidate["latest_incident"]:
                return None
            _record(db, _valid_points(snapshot), now, from_shadow=True)
        job = _pending(db, profile, now)
        if job is None:
            return None
        if job["late"] and verify_late and (
                not candidate["late"] or job["latest_incident"] != candidate["latest_incident"]):
            return None
        db.execute("""
            INSERT INTO deliveries(incident,profile,ordinal,kind,hazards,reported_at,started_at,status,attempt)
            VALUES(:incident,:profile,:ordinal,:kind,:hazards,:reported_at,:started_at,'reserved',:attempt)
            ON CONFLICT(incident,profile,ordinal) DO UPDATE SET hazards=excluded.hazards,
              reported_at=excluded.reported_at, started_at=excluded.started_at, status='reserved',
              receipt=NULL, attempt=excluded.attempt, retry_at=NULL
        """, job)
        return job


def finish(db, job, status, receipt, completed_at=None):
    retry_at = None
    if status == "failed":
        when = now_ms() if completed_at is None else completed_at
        retry_at = when + RETRY_DELAYS[min(job["attempt"], len(RETRY_DELAYS)) - 1]
    db.execute("""UPDATE deliveries SET status=?, receipt=?, retry_at=?
                  WHERE incident=? AND profile=? AND ordinal=? AND attempt=? AND status='reserved'""",
               (status, receipt, retry_at, job["incident"], job["profile"], job["ordinal"], job["attempt"]))
