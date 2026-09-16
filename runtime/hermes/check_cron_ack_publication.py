"""Offline deterministic probe of the candidate core's ready-file publication.

Runs only the extracted acknowledgement-writing statements with synthetic state.
No scheduler, job, profile, network, or production database is opened.
An exposed incomplete JSON file is a release blocker, not proof of a lost job.
"""
from __future__ import annotations

import argparse
import ast
import json
import os
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace


def probe(core: Path) -> bool:
    tree = ast.parse((core / "cron/scheduler.py").read_text())
    candidates = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Try) and any(
            isinstance(statement, ast.Assign)
            and isinstance(statement.value, ast.Call)
            and isinstance(statement.value.func, ast.Attribute)
            and statement.value.func.attr == "open"
            and any(isinstance(item, ast.Name) and item.id == "ack_path" for item in ast.walk(statement.value))
            for statement in node.body
        ):
            candidates.append(node)
    # Do not silently pass an unrecognized future implementation.
    if len(candidates) != 1:
        raise RuntimeError("ack_publication_shape_requires_review")
    code = compile(ast.Module(body=candidates[0].body, type_ignores=[]), "candidate-ack-publication", "exec")
    reached = threading.Event()
    resume = threading.Event()
    failures = []
    def delayed_dump(*args, **kwargs):
        reached.set()
        if not resume.wait(10):
            raise RuntimeError("probe_timeout")
        return json.dump(*args, **kwargs)
    with tempfile.TemporaryDirectory(prefix="hermes-ack-probe-") as temporary:
        ack = Path(temporary) / "fixture.ready"
        def writer():
            try:
                exec(code, {"os": os, "json": SimpleNamespace(dump=delayed_dump), "ack_path": ack, "execution_id": "fixture-execution"})
            except BaseException as exc:
                failures.append(type(exc).__name__)
        thread = threading.Thread(target=writer)
        thread.start()
        exposed = False
        try:
            if not reached.wait(10):
                raise RuntimeError("publication_not_reached")
            if ack.exists():
                try:
                    json.loads(ack.read_text())
                except json.JSONDecodeError:
                    exposed = True
        finally:
            resume.set()
            thread.join(10)
        if thread.is_alive() or failures:
            raise RuntimeError("publication_probe_failed")
        assert json.loads(ack.read_text())["execution_id"] == "fixture-execution"
        return not exposed


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-dir", type=Path, required=True)
    args = parser.parse_args()
    passed = probe(args.core_dir.resolve(strict=True))
    print("cron_ack_atomic_publication=" + ("passed" if passed else "blocked_incomplete_json_visible"))
    raise SystemExit(0 if passed else 1)
