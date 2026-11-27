"""Durable alarm state for the single fireplace detector and its two recipients."""

import sqlite3
import time
from contextlib import contextmanager
from pathlib import Path


DATABASE = Path("/var/lib/qintopia-tuya-alerts/state.db")
SIGNALS = {"smoke_sensor_state": 1, "co_state": 2}
PROFILES = ("silaoshi", "erhua")
INTERVAL = 30_000


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
def transaction(db):
    db.execute("BEGIN IMMEDIATE")
    try:
        yield
        db.commit()
    except BaseException:
        db.rollback()
        raise


def initialize(db, armed_at):
    db.executescript("""
        CREATE TABLE IF NOT EXISTS installation (
            id INTEGER PRIMARY KEY CHECK (id=1), armed_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS signals (
            code TEXT PRIMARY KEY, value TEXT NOT NULL, reported_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS incidents (
            id INTEGER PRIMARY KEY, active INTEGER NOT NULL, hazards INTEGER NOT NULL,
            reported_at INTEGER NOT NULL);
        CREATE UNIQUE INDEX IF NOT EXISTS one_open ON incidents((1)) WHERE active != 0;
        CREATE TABLE IF NOT EXISTS routes (
            incident INTEGER REFERENCES incidents(id), profile TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0 CHECK (count BETWEEN 0 AND 3),
            first_at INTEGER, due INTEGER, PRIMARY KEY(incident,profile));
        CREATE INDEX IF NOT EXISTS pending ON routes(profile,due) WHERE due IS NOT NULL;
        CREATE TABLE IF NOT EXISTS deliveries (
            incident INTEGER, profile TEXT, ordinal INTEGER, kind TEXT NOT NULL,
            hazards INTEGER NOT NULL, reported_at INTEGER NOT NULL, started_at INTEGER NOT NULL,
            status TEXT NOT NULL, receipt TEXT,
            PRIMARY KEY(incident,profile,ordinal),
            FOREIGN KEY(incident,profile) REFERENCES routes(incident,profile));
    """)
    db.execute("INSERT OR IGNORE INTO installation VALUES(1,?)", (armed_at,))


def apply(db, points, received):
    candidates = [p for p in points if p.get("code") in SIGNALS
                  and p.get("value") in ("alarm", "normal") and type(p.get("time")) is int]
    if not candidates:
        return
    with transaction(db):
        cutoff = db.execute("SELECT armed_at FROM installation WHERE id=1").fetchone()[0]
        updated = False
        for p in candidates:
            if p["time"] < cutoff:
                continue
            # A re-delivery cannot reverse a more recent value. At equal time keep alarm.
            change = db.execute("""
                INSERT INTO signals VALUES(?,?,?) ON CONFLICT(code) DO UPDATE
                SET value=excluded.value, reported_at=excluded.reported_at
                WHERE excluded.reported_at > signals.reported_at OR
                  (excluded.reported_at=signals.reported_at AND
                   excluded.value='alarm' AND signals.value='normal')
            """, (p["code"], p["value"], p["time"]))
            updated |= bool(change.rowcount)
        if not updated:
            return
        signals = db.execute("SELECT * FROM signals").fetchall()
        active = sum(SIGNALS[p["code"]] for p in signals if p["value"] == "alarm")
        incident = db.execute("SELECT * FROM incidents WHERE active != 0").fetchone()
        if not active and incident is None:
            return

        hazards = active | (incident["hazards"] if incident else 0)
        reported = max(p["reported_at"] for p in signals if SIGNALS[p["code"]] & (active or hazards))
        if incident is None:
            # An older, unsent recovery would contradict this new incident.
            db.execute("UPDATE routes SET due=NULL WHERE due IS NOT NULL")
            number = db.execute("INSERT INTO incidents(active,hazards,reported_at) VALUES(?,?,?)",
                                (active, hazards, reported)).lastrowid
            db.executemany("INSERT INTO routes(incident,profile,due) VALUES(?,?,?)",
                           [(number, role, received) for role in PROFILES])
            return

        number = incident["id"]
        db.execute("UPDATE incidents SET active=?,hazards=?,reported_at=? WHERE id=?",
                   (active, hazards, reported, number))
        if not active:
            db.execute("UPDATE routes SET due=CASE WHEN count>0 THEN ? END WHERE incident=?",
                       (received, number))
        elif active & ~incident["active"]:
            db.execute("UPDATE routes SET due=? WHERE incident=? AND count<3", (received, number))


DUE = """
    SELECT r.*, i.active, i.hazards, i.reported_at FROM routes AS r
    JOIN incidents AS i ON i.id=r.incident
    WHERE r.profile=? AND r.due<=? ORDER BY r.due, r.incident LIMIT 1
"""


def claim(db, profile, now, verify_late=None):
    # Normal idle ticks are read-only. Recheck under the write lock before reserving.
    args = (profile, now)
    pending = db.execute(DUE, args).fetchone()
    if pending is None:
        return None
    if (verify_late and pending["active"] and
            (now - pending["due"] > 60_000 or
             (pending["count"] == 0 and now - pending["reported_at"] > 60_000))):
        # After a service outage a newer device-reported normal can already be
        # available while the corresponding subscription event is still queued.
        apply(db, verify_late(pending["active"]), now)
    with transaction(db):
        route = db.execute(DUE, args).fetchone()
        if route is None:
            return None
        key = (route["incident"], profile)
        first_at = route["first_at"]
        count = route["count"]
        due = None
        if route["active"]:
            count += 1
            first_at = now if first_at is None else first_at
            if count < 3:
                due = max(first_at + INTERVAL * count, now + INTERVAL)
            ordinal, kind, hazards = count, "alarm", route["active"]
        else:
            notified = db.execute("""
                SELECT 1 FROM deliveries WHERE incident=? AND profile=? AND kind='alarm'
                AND status='accepted' LIMIT 1
            """, key).fetchone()
            if notified is None:
                db.execute("UPDATE routes SET due=NULL WHERE incident=? AND profile=?", key)
                return None
            ordinal, kind, hazards = 4, "recovery", route["hazards"]
        db.execute("UPDATE routes SET count=?, first_at=?, due=? WHERE incident=? AND profile=?",
                   (count, first_at, due, *key))
        # Reserve before network I/O. Restarting cannot send the same slot twice.
        job = dict(incident=key[0], profile=profile, ordinal=ordinal, kind=kind,
                   hazards=hazards, reported_at=route["reported_at"], started_at=now)
        db.execute("""
            INSERT INTO deliveries(incident,profile,ordinal,kind,hazards,reported_at,started_at,status)
            VALUES(:incident,:profile,:ordinal,:kind,:hazards,:reported_at,:started_at,'reserved')
        """, job)
        return job


def finish(db, job, status, receipt):
    db.execute("""
        UPDATE deliveries SET status=?,receipt=? WHERE incident=? AND profile=? AND ordinal=?
    """, (status, receipt, job["incident"], job["profile"], job["ordinal"]))
