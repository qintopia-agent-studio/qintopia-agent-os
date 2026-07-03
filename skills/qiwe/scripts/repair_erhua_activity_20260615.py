import json
import os
import sys
from pathlib import Path
from datetime import datetime, timezone


def load_env(profile: Path) -> None:
    env_path = profile / ".env"
    if env_path.exists():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped or stripped.startswith("#") or "=" not in stripped:
                continue
            key, value = stripped.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip().strip("'\""))
    dropin = Path("/home/ubuntu/.config/systemd/user/hermes-gateway-erhua.service.d/10-profile-env.conf")
    if dropin.exists():
        for line in dropin.read_text(encoding="utf-8").splitlines():
            stripped = line.strip()
            if not stripped.startswith("Environment="):
                continue
            payload = stripped[len("Environment="):].strip().strip('"')
            if "=" not in payload:
                continue
            key, value = payload.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip().strip("'\""))


def main() -> None:
    profile = Path("/home/ubuntu/.hermes/profiles/erhua")
    plugin = profile / "plugins" / "qiwe-platform"
    os.chdir(plugin)
    sys.path.insert(0, str(plugin))
    load_env(profile)

    from solitaire.activity_service import ActivityService
    from solitaire.feishu_writer import FeishuActivityWriter
    from solitaire.parser import ActivityRecord
    from solitaire.repository import ActivityRepository

    base = profile / "solitaire"
    canonical = "act_80ab674724c23995f8c1"
    duplicates = ["act_704badc7b6e06a69ac25", "act_57a11b241d23302d14cc"]
    duplicate_record_ids = ["recvmzvJH0s6LG"]
    correct_start = "2026-06-15 14:30"
    now = datetime.now(timezone.utc).isoformat()

    activities_path = base / "activities.json"
    reminders_path = base / "reminders.json"
    record_ids_path = base / "feishu_record_ids.json"
    activities = json.loads(activities_path.read_text(encoding="utf-8") or "{}")
    reminders = json.loads(reminders_path.read_text(encoding="utf-8") or "{}") if reminders_path.exists() else {}
    record_ids = json.loads(record_ids_path.read_text(encoding="utf-8") or "{}") if record_ids_path.exists() else {}

    if canonical not in activities:
        raise SystemExit(f"missing canonical activity {canonical}")
    activity = activities[canonical]
    activity["start_time"] = correct_start
    activity["status"] = "active"
    activity["last_seen_at"] = now
    activity["activity_detail"] = (
        "时间：6/15（周一）下午2:30；地点：秦托邦一楼大厅。"
        "适合零基础，材料已备好，人来了就行。不只是做一件小作品，更是让自己慢下来、静下来的一小段时光。"
    )
    activity["promo_text"] = "今天下午2:30，来秦托邦一楼大厅和贺妈妈一起穿针引线，享受一段慢下来、静下来的手作时光。"
    activity["time_normalization_note"] = (
        "接龙原文写作 4/15（周一）下午2:30，但消息发送时已是 2026-06-15；"
        "已按当前月份记录为 2026-06-15 14:30。"
    )

    for duplicate in duplicates:
        activities.pop(duplicate, None)
        record_ids.pop(duplicate, None)
    for job_id, job in list(reminders.items()):
        if isinstance(job, dict) and job.get("activity_id") in set(duplicates + [canonical]):
            reminders.pop(job_id, None)

    activities[canonical] = activity
    activities_path.write_text(json.dumps(activities, ensure_ascii=False, sort_keys=True), encoding="utf-8")
    record_ids_path.write_text(json.dumps(record_ids, ensure_ascii=False, sort_keys=True), encoding="utf-8")
    reminders_path.write_text(json.dumps(reminders, ensure_ascii=False, sort_keys=True), encoding="utf-8")

    service = ActivityService(ActivityRepository(str(profile)), FeishuActivityWriter.from_env(), None)
    record_kwargs = {key: activity.get(key) for key in ActivityRecord.__dataclass_fields__ if key in activity}
    record = ActivityRecord(**record_kwargs)
    service.repository.upsert_reminders(record)
    result = service.sync_feishu(canonical)

    writer = service.writer
    app_token = os.environ.get(writer.mapping.app_token_env, "").strip()
    table_id = os.environ.get(writer.mapping.table_id_env, "").strip()
    token = writer._tenant_access_token()
    deleted = []
    for record_id in duplicate_record_ids:
        try:
            response = writer._request(
                f"/bitable/v1/apps/{app_token}/tables/{table_id}/records/{record_id}",
                token,
                {},
                method="DELETE",
                body=False,
            )
            deleted.append({"record_id": record_id, "code": response.get("code"), "msg": response.get("msg", "")})
        except Exception as exc:
            deleted.append({"record_id": record_id, "error": str(exc)})

    activities = json.loads(activities_path.read_text(encoding="utf-8") or "{}")
    reminders = json.loads(reminders_path.read_text(encoding="utf-8") or "{}") if reminders_path.exists() else {}
    final_reminders = {
        job_id: job
        for job_id, job in reminders.items()
        if isinstance(job, dict) and job.get("activity_id") == canonical
    }
    print(json.dumps({
        "sync_success": result.success,
        "sync_error": result.error,
        "sync_record_id": result.record_id,
        "deleted": deleted,
        "activity": {
            key: activities[canonical].get(key)
            for key in [
                "activity_id",
                "start_time",
                "participant_count",
                "participant_names",
                "source_message_id",
                "last_seen_at",
                "time_normalization_note",
            ]
        },
        "reminders": final_reminders,
    }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
