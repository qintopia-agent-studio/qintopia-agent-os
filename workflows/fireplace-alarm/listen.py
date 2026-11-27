"""Connect the registered fireplace device's push events to the durable workflow."""

import argparse
import logging
import sys
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parent
sys.path.insert(0, str(WORKFLOW))
sys.path.insert(0, str(WORKFLOW.parents[1] / "mcp/qintopia-sensors"))
import state
import subscription


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen", action="store_true", help="join the configured Tuya subscription")
    if not parser.parse_args().listen:
        parser.error("No subscription started. Use --listen only for a reviewed cutover.")
    device_id = subscription.fireplace_id()
    db = state.connect(create=True)
    try:
        state.initialize(db, state.now_ms())
        subscription.consume(device_id, lambda points: state.apply(db, points, state.now_ms()))
    finally:
        db.close()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    main()
