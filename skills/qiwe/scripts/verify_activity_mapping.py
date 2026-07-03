import json
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from solitaire.feishu_writer import FeishuActivityWriter


def _load_systemd_dropin_env(path: str) -> None:
    import os

    if not path:
        return
    dropin = Path(path).expanduser()
    if not dropin.exists():
        raise SystemExit(f"systemd drop-in not found: {dropin}")
    for line in dropin.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped.startswith("Environment="):
            continue
        payload = stripped[len("Environment=") :].strip().strip('"')
        if "=" not in payload:
            continue
        key, value = payload.split("=", 1)
        if key:
            os.environ.setdefault(key, value)


def _activity_payload(version: str) -> dict:
    participant_names = ["弦默", "阿凯", "大羽"] if version == "create" else ["弦默", "阿凯", "大羽", "二花测试更新"]
    return {
        "activity_id": "act_feishu_live_probe",
        "source_group_id": "10789255155259073",
        "source_message_id": f"msg_probe_{version}",
        "source_sender_id": "sender_probe",
        "activity_subject": "二花 Feishu live upsert 测试",
        "activity_detail": f"用于验证飞书 create/update 幂等写入：{version}",
        "start_time": "2026-06-12 21:30:00",
        "solitaire_created_at": "2026-06-12 21:10:00",
        "participant_count": len(participant_names),
        "participant_names": participant_names,
        "promo_text": "这是一条二花飞书写入测试记录。",
        "last_seen_at": "2026-06-12 21:20:00",
        "status": "active",
        "raw_summary": f"live upsert probe {version}",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--load-systemd-dropin", default="", help="Optional systemd drop-in file to load Environment= values from.")
    parser.add_argument("--live-probe", action="store_true", help="Write a fixed test activity twice to verify live upsert.")
    args = parser.parse_args()

    _load_systemd_dropin_env(args.load_systemd_dropin)

    writer = FeishuActivityWriter.from_env()
    probe = writer.probe_fields()
    first = writer.write(_activity_payload("create"))
    second = writer.write(_activity_payload("update")) if args.live_probe and first.success else None
    print(
        json.dumps(
            {
                "probe_success": probe.success,
                "probe_error": probe.error,
                "field_count": len(probe.fields),
                "field_names": [field.field_name for field in probe.fields],
                "first_write": {
                    "success": first.success,
                    "skipped": first.skipped,
                    "mode": first.mode,
                    "record_id": first.record_id,
                    "error": first.error,
                    "mapped_fields": first.mapped_fields,
                },
                "second_write": None
                if second is None
                else {
                    "success": second.success,
                    "skipped": second.skipped,
                    "mode": second.mode,
                    "record_id": second.record_id,
                    "error": second.error,
                    "mapped_fields": second.mapped_fields,
                },
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
