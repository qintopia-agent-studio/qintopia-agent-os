"""Opt-in real Hermes registry -> production broker probe in isolated Linux.

The Rust fixture owns the disposable identities, messages and broker. This client
receives no database credentials. No model, gateway or external sends are used.
"""
from __future__ import annotations

import argparse
import inspect
import json
import os
from pathlib import Path
import socket
import sys


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--phase", choices=("positive", "same_uid", "wrong_uid", "revoked", "drain_timeout", "drain_replay"), required=True)
    args = parser.parse_args()
    data = json.loads(args.input.read_text())
    assert sys.platform == "linux", "isolated_linux_required"
    assert "QINTOPIA_FOUNDATION_DATABASE_URL" not in os.environ, "client_database_credentials_forbidden"
    assert "QINTOPIA_COLLABORATION_TEST_DATABASE_URL" not in os.environ, "client_database_credentials_forbidden"
    assert os.getuid() == data["uids"][args.phase], "unexpected_client_uid"
    core = Path(os.environ["QINTOPIA_HERMES_CORE_DIR"]).resolve(strict=True)
    sys.path.insert(0, str(core))
    from hermes_cli.plugins import get_plugin_manager
    from tools.registry import registry
    manager = get_plugin_manager()
    manager.discover_and_load()
    expected = {"context", "workspace", "change_knowledge", "remember", "history", "task_status", "delegate_review", "candidates"}
    registered = {name.removeprefix("qintopia_person_") for name in manager._plugin_tool_names if name.startswith("qintopia_person_")}
    assert registered == expected, "minimal_registration_mismatch"
    for tool in expected:
        entry = registry.get_entry("qintopia_person_" + tool, scope=manager.scope_key)
        assert entry is not None, "official_registry_entry_missing"
        assert Path(inspect.getsourcefile(entry.handler)).resolve() == Path(data["sdk"]).resolve(), "actual_sdk_handler_required"
    assert type(manager).__module__ == "hermes_cli.plugins", "official_plugin_manager_required"
    checks = ["official_hermes_discovery", "actual_erhua_minimal_registration", "actual_sdk_handlers"]

    def call(tool: str, arguments: dict, event: str = "context", **session) -> dict:
        os.environ.update({
            "HERMES_SESSION_PLATFORM": "qiwe",
            "HERMES_SESSION_CONVERSATION_TYPE": "direct",
            "HERMES_SESSION_CHAT_ID": "simulated-linux-direct",
            "HERMES_SESSION_USER_ID": "synthetic-resident",
            "HERMES_SESSION_MESSAGE_ID": data["events"][event]["message"],
            **session,
        })
        raw = registry.dispatch("qintopia_person_" + tool, arguments, scope=manager.scope_key)
        return json.loads(raw) if isinstance(raw, str) else raw

    def ok(tool: str, arguments: dict, event: str = "context", **session) -> dict:
        result = call(tool, arguments, event, **session)
        assert result.get("ok") is True, "tool_failed_" + result.get("error", {}).get("code", "unknown")
        return result["result"]

    def denied(tool: str, arguments: dict, event: str = "context", **session) -> dict:
        result = call(tool, arguments, event, **session)
        assert result.get("ok") is False and "result" not in result, "unauthorized_result"
        return result

    def change(event: str, version: int, action: str, text: str = "") -> dict:
        edit = {"action": action}
        if action == "save":
            edit["content"] = {"title": "模拟 Linux 本栋约定", "text": text}
        return {"operation_id": data["events"][event]["operation"], "expected_version": version,
                "key": "linux-probe", "kind": "rule", "change": edit}

    if args.phase in {"same_uid", "wrong_uid"}:
        result = denied("context", {})
        expected_code = "foundation_unavailable" if args.phase == "same_uid" else "outcome_unknown"
        assert result["error"]["code"] == expected_code, "peer_rejection_missing"
        # Also verify SO_PEERCRED rejection independently of the SDK's same-UID guard.
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(5)
            client.connect(os.environ["QINTOPIA_FOUNDATION_SOCKET"])
            try:
                client.sendall(b"{}\n")
                assert client.recv(1) == b"", "unauthorized_peer_readable"
            except (BrokenPipeError, ConnectionResetError):
                pass
        checks.append("actual_peer_credentials_rejected")
    elif args.phase == "drain_timeout":
        result = denied("change_knowledge", {**change("drain", 0, "save", "超时后仍完成保存"), "key": "linux-drain"}, "drain")
        assert result["error"]["code"] == "outcome_unknown", "accepted_work_timeout_not_preserved"
        checks.append("accepted_database_work_client_timeout")
    elif args.phase == "drain_replay":
        saved = ok("change_knowledge", {**change("drain", 0, "save", "超时后仍完成保存"), "key": "linux-drain"}, "drain")
        assert saved["replayed"] is True, "original_operation_receipt_not_recovered"
        checks.append("restart_original_operation_receipt_recovered")
    elif args.phase == "revoked":
        denied("change_knowledge", change("revoked", 3, "save", "撤权后不能保存"), "revoked")
        checks.append("revoked_authority_denied")
    else:
        context = ok("context", {})
        assert context["identity"]["identity_status"] == "confirmed", "trusted_identity_missing"
        checks.append("trusted_ingress_identity")
        saved = ok("change_knowledge", change("save", 0, "save", "晚上十点关闭"), "save")
        assert saved["status"] == "saved", "save_receipt_missing"
        assert ok("context", {})["knowledge"][0]["content"]["text"] == "晚上十点关闭", "save_readback_missing"
        ok("change_knowledge", change("update", 1, "save", "晚上九点关闭"), "update")
        assert ok("context", {})["knowledge"][0]["content"]["text"] == "晚上九点关闭", "update_readback_missing"
        denied("change_knowledge", change("stale", 1, "save", "旧版本不能覆盖"), "stale")
        ok("change_knowledge", change("stop", 2, "stop"), "stop")
        assert ok("context", {})["knowledge"] == [], "stopped_knowledge_still_used"
        assert ok("change_knowledge", change("save", 0, "save", "晚上十点关闭"), "save")["replayed"] is True, "receipt_replay_missing"
        assert ok("context", {})["knowledge"] == [], "replay_restored_stopped_knowledge"
        checks += ["knowledge_save_readback", "knowledge_update_readback", "stale_version_denied", "knowledge_stop_and_replay"]
        version = ok("context", {})["memory"]["version"]
        first = {"operation_id": data["events"]["memory_general"]["operation"], "expected_version": version,
                 "change": {"action": "set", "condition": "general", "style": "brief"}}
        general = ok("remember", first, "memory_general")
        fees = ok("remember", {"operation_id": data["events"]["memory_fees"]["operation"], "expected_version": general["version"],
                               "change": {"action": "set", "condition": "fees", "style": "detailed"}}, "memory_fees")
        assert ok("context", {"topic": "general"})["memory"]["reply_style"] == "brief"
        assert ok("context", {"topic": "fees"})["memory"]["reply_style"] == "detailed"
        ok("remember", {"operation_id": data["events"]["memory_stop"]["operation"], "expected_version": fees["version"], "change": {"action": "stop"}}, "memory_stop")
        assert ok("context", {})["memory"]["reply_style"] is None
        assert ok("remember", first, "memory_general")["replayed"] is True
        assert ok("context", {})["memory"]["reply_style"] is None
        checks.append("conditional_memory_save_readback_stop_and_replay")
        denied("context", {"actor": "forged"})
        denied("change_knowledge", {**change("forged", 3, "save", "伪造来源"), "scope": data["foreign_scope"]}, "forged")
        denied("change_knowledge", change("forged", 3, "save", "未认证来源"), "forged")
        denied("context", {}, HERMES_SESSION_MESSAGE_ID="simulated-not-persisted")
        denied("context", {}, HERMES_SESSION_CONVERSATION_TYPE="group", HERMES_SESSION_CHAT_ID=data["foreign_chat"])
        token = os.environ["QINTOPIA_FOUNDATION_TOKEN"]
        os.environ["QINTOPIA_FOUNDATION_TOKEN"] = "simulated-wrong-token-xxxxxxxxxxxxxxxx"
        denied("context", {})
        os.environ["QINTOPIA_FOUNDATION_TOKEN"] = token
        checks += ["privileged_arguments_denied", "unauthenticated_or_unknown_source_denied", "cross_scope_denied", "wrong_token_denied"]
    print(json.dumps({"phase": args.phase, "checks": checks, "count": len(checks), "uid": os.getuid(), "gid": os.getgid(),
                      "core_sha": data["core_sha"], "official_registry": True, "actual_unix_socket": True,
                      "simulated_identities": True, "real_llm": False, "external_send": False}))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Emit only stable failures, never tokens, session IDs or tool bodies.
        message = str(error)
        if not message or len(message) > 120 or not all(c.isalnum() or c == "_" for c in message):
            message = "probe_failed"
        print(json.dumps({"passed": False, "error_type": type(error).__name__, "code": message}), file=sys.stderr)
        raise SystemExit(1)
