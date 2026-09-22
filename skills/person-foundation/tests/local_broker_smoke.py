"""Opt-in synthetic SDK -> authenticated Rust broker -> PostgreSQL smoke.

Run only against this batch's disposable local fixture. Never prints identity/token data.
"""
from __future__ import annotations
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlparse
from uuid import uuid4


def main() -> None:
    if os.environ.get("QINTOPIA_FOUNDATION_SMOKE_ENABLE") != "1" or not os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID", "").startswith("synthetic-"):
        raise RuntimeError("explicit_synthetic_smoke_required")
    path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("foundation_actual_broker", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    database = urlparse(os.environ.get("QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL", ""))
    tenant = os.environ.get("QINTOPIA_COLLABORATION_LOCAL_TENANT", "")
    if database.hostname != "127.0.0.1" or database.path != "/qintopia_test" or not database.port or not re.fullmatch(r"synthetic-collaboration-[a-z0-9-]+", tenant):
        raise RuntimeError("isolated_fixture_database_required")
    containers = subprocess.check_output(["docker", "ps", "--format", "{{.Names}}\t{{.Ports}}"], text=True)
    matches = [line.split("\t", 1)[0] for line in containers.splitlines() if f"127.0.0.1:{database.port}->5432/tcp" in line]
    if len(matches) != 1:
        raise RuntimeError("isolated_fixture_container_required")
    def seed(source, *, old=False):
        # Model-visible messages are not retained; only synthetic provenance and
        # an authenticated-ingress marker exercise the real Rust evidence lookup.
        sql = """
        WITH source AS (
          INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified)
          VALUES(:'source','qiwe','qintopia.qiwe.raw.authenticated',clock_timestamp(),'{}',true)
          ON CONFLICT(source,event_id) DO UPDATE SET event_id=EXCLUDED.event_id RETURNING id
        ) INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,sent_at,received_at,raw_event_id,raw)
          SELECT :'tenant','qiwe',:'source',:'source','synthetic-resident','direct','synthetic-resident','text',
            clock_timestamp() - CASE WHEN :'old'='true' THEN interval '1 day' ELSE interval '0 seconds' END,
            clock_timestamp(),id,'{}' FROM source ON CONFLICT(platform,message_id) DO NOTHING;
        """
        subprocess.run(["docker", "exec", "-i", matches[0], "psql", "-X", "-U", "postgres", "-d", "qintopia_test", "-v", "ON_ERROR_STOP=1",
                        "-v", "tenant=" + tenant, "-v", "source=" + source, "-v", "old=" + str(old).lower(), "-f", "-"],
                       input=sql, text=True, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    message = str(uuid4())
    session = {"platform": "qiwe", "conversation_type": "direct", "conversation_id": "synthetic-resident",
               "requester_user_id": "synthetic-resident", "source_message_id": message}
    results = []
    def invoke(tool, arguments, source=None):
        session["source_message_id"] = source or str(uuid4())
        seed(session["source_message_id"])
        response = module.invoke(tool, arguments, agent_id="erhua", session_provider=lambda: session.copy())
        if response.get("ok") is not True:
            raise RuntimeError("broker_smoke_failed_" + response.get("error", {}).get("code", "unknown"))
        return response["result"]
    state = invoke("context", {"purpose": "reply", "topic": "general"})
    assert state["identity"]["identity_status"] == "confirmed"
    results.append("sdk_broker_trusted_identity")
    first = {"operation_id": str(uuid4()), "expected_version": state["memory"]["version"],
             "change": {"action": "set", "condition": "general", "style": "brief"}}
    saved = invoke("remember", first, message)
    assert saved["saved"] is True
    state = invoke("context", {"topic": "general"})
    assert state["memory"]["reply_style"] == "brief"
    results.append("durable_memory_readback")
    saved = invoke("remember", {"operation_id": str(uuid4()), "expected_version": saved["version"],
        "change": {"action": "set", "condition": "fees", "style": "detailed"}})
    assert invoke("context", {"topic": "fees"})["memory"]["reply_style"] == "detailed"
    assert invoke("context", {"topic": "general"})["memory"]["reply_style"] == "brief"
    results.append("conditional_memory")
    invoke("remember", {"operation_id": str(uuid4()), "expected_version": saved["version"], "change": {"action": "stop"}})
    assert invoke("context", {})["memory"]["reply_style"] is None
    result = invoke("remember", first, message)
    assert result["replayed"] is True
    assert invoke("context", {})["memory"]["status"] == "stopped"
    results.append("old_message_receipt_never_restores_stopped_memory")
    late_source = str(uuid4())
    seed(late_source, old=True)
    session["source_message_id"] = late_source
    stopped = invoke("context", {})["memory"]
    session["source_message_id"] = late_source
    late = module.invoke("remember", {"operation_id": str(uuid4()), "expected_version": stopped["version"],
        "change": {"action": "set", "condition": "general", "style": "brief"}}, agent_id="erhua", session_provider=lambda: session.copy())
    assert late["error"]["code"] == "memory_stale_source"
    assert invoke("context", {})["memory"]["status"] == "stopped"
    results.append("unseen_late_message_cannot_restore_memory")
    bad = module.invoke("context", {"actor": "injected"}, agent_id="erhua", session_provider=lambda: session)
    assert bad["error"]["code"] == "invalid_arguments"
    context = module.trusted_context(lambda: session)
    request = {"operation": "person_foundation_tool", "schema_version": 1, "agent": "erhua",
               "tool": "context", "trusted_context": context, "arguments": {"scope": str(uuid4())}}
    assert module.socket_call(request)["error"]["code"] == "invalid_arguments"
    results.append("client_and_broker_privileged_argument_rejection")
    request["arguments"] = {}
    request["trusted_context"] = {**context, "chat_type": "group", "chat_id": "synthetic-unbound-group"}
    assert module.socket_call(request)["error"]["code"] == "gateway_scope_mismatch"
    request["trusted_context"] = {**context, "sender_id": "synthetic-unconfirmed-person"}
    assert module.socket_call(request)["error"]["code"] == "gateway_identity_unconfirmed"
    request["trusted_context"] = context
    request["agent"] = "silaoshi"
    assert module.socket_call(request)["error"]["code"] == "agent_tool_denied"
    results.append("broker_scope_identity_and_profile_rejection")
    print(json.dumps({"passed": results, "count": len(results), "transport": "actual_local_unix_broker",
        "persistence": "disposable_postgresql", "synthetic": True, "real_llm_tested": False,
        "external_send_or_upload": False}, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Runtime exceptions may contain OS details; emit only a stable failure class.
        code = str(error) if re.fullmatch(r"[a-z_]{1,120}", str(error)) else "verification_failed"
        print(json.dumps({"passed": False, "error_type": type(error).__name__, "code": code}), file=sys.stderr)
        sys.exit(1)
