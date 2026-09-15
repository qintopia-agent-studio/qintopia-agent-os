from __future__ import annotations

import asyncio
import hashlib
import json
import os
import struct
import tempfile
import unittest
import uuid
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

import space_agent_completion as completion


TOKEN = "dedicated-runner-token-with-at-least-thirty-two-bytes"
TOKEN_SHA256 = hashlib.sha256(TOKEN.encode("utf-8")).hexdigest()
CAPABILITY_KEY = "erhua.space_subject_identity_lookup"


def capability_descriptor() -> dict:
    return {
        "capability_key": CAPABILITY_KEY,
        "input_schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {},
            "required": [],
        },
        "output_schema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {"members": {"type": "array"}},
            "required": ["members"],
        },
        "risk_level": "low",
        "review_policy": "definition_policy",
    }


def request_payload(*, history: list[dict] | None = None) -> dict:
    return {
        "operation": "space_agent_turn_complete",
        "schema_version": 1,
        "runner_identity": completion.RUNNER_IDENTITY,
        "runner_token": TOKEN,
        "work_item_id": str(uuid.uuid4()),
        "goal": "Return one bounded summary.",
        "trigger": {
            "kind": "event",
            "event_type": "group_member_add",
            "subject_user_ids": ["9007199254740993"],
        },
        "output_contract": {
            "type": "object",
            "additionalProperties": False,
            "properties": {"summary": {"type": "string"}},
            "required": ["summary"],
        },
        "capabilities": [capability_descriptor()],
        "history": history or [],
    }


class FakeLlm:
    def __init__(self, decision: dict):
        self.decision = decision
        self.calls: list[dict] = []

    async def acomplete(self, **kwargs):
        self.calls.append(kwargs)
        return SimpleNamespace(text=json.dumps(self.decision))


class FakePeerSocket:
    def __init__(self, pid: int, uid: int, gid: int):
        self.credentials = struct.pack("3i", pid, uid, gid)

    def getsockopt(self, level: int, option: int, length: int) -> bytes:
        self.request = (level, option, length)
        return self.credentials


class FakeWriter:
    def __init__(self, peer_socket):
        self.peer_socket = peer_socket

    def get_extra_info(self, name: str):
        return self.peer_socket if name == "socket" else None


class RecordingConnectionWriter:
    def __init__(self):
        self.output = bytearray()
        self.closed = False

    def write(self, value: bytes) -> None:
        self.output.extend(value)

    async def drain(self) -> None:
        return None

    def close(self) -> None:
        self.closed = True

    async def wait_closed(self) -> None:
        return None


class FakeAsyncServer:
    def __init__(self):
        self.closed = False

    def close(self) -> None:
        self.closed = True

    async def wait_closed(self) -> None:
        return None


class SpaceAgentCompletionConfigTests(unittest.TestCase):
    def test_disabled_by_default_without_any_secret_or_model(self):
        with patch.dict(os.environ, {}, clear=True):
            config = completion.SpaceAgentCompletionConfig.from_environment()
        self.assertFalse(config.enabled)

    def test_enablement_requires_exact_owner_hash_ids_and_absolute_socket(self):
        valid = {
            completion.ENABLE_ENV: "1",
            completion.APPROVAL_ENV: completion.APPROVAL_PHRASE,
            completion.TOKEN_SHA256_ENV: TOKEN_SHA256,
            completion.RUNNER_UID_ENV: "2001",
            completion.RUNNER_GID_ENV: "2002",
            completion.SOCKET_ENV: "/run/qintopia-agentos-agent-turn/completion.sock",
        }
        with patch.dict(os.environ, valid, clear=True):
            config = completion.SpaceAgentCompletionConfig.from_environment()
        self.assertTrue(config.enabled)
        self.assertEqual(config.runner_uid, 2001)
        self.assertEqual(config.runner_gid, 2002)

        for key, value in [
            (completion.APPROVAL_ENV, "wrong"),
            (completion.TOKEN_SHA256_ENV, "not-a-hash"),
            (completion.RUNNER_UID_ENV, "0"),
            (completion.RUNNER_GID_ENV, "0"),
            (completion.SOCKET_ENV, "relative.sock"),
        ]:
            invalid = {**valid, key: value}
            with self.subTest(key=key), patch.dict(os.environ, invalid, clear=True):
                with self.assertRaises(ValueError):
                    completion.SpaceAgentCompletionConfig.from_environment()

    def test_invalid_enablement_and_timeout_fail_closed(self):
        with patch.dict(os.environ, {completion.ENABLE_ENV: "true"}, clear=True):
            with self.assertRaises(ValueError):
                completion.SpaceAgentCompletionConfig.from_environment()
        env = {
            completion.ENABLE_ENV: "1",
            completion.APPROVAL_ENV: completion.APPROVAL_PHRASE,
            completion.TOKEN_SHA256_ENV: TOKEN_SHA256,
            completion.RUNNER_UID_ENV: "2001",
            completion.RUNNER_GID_ENV: "2002",
            completion.SOCKET_ENV: "/run/qintopia-agentos-agent-turn/completion.sock",
            "QIWE_SPACE_AGENT_COMPLETION_TIMEOUT_SECONDS": "61",
        }
        with patch.dict(os.environ, env, clear=True):
            with self.assertRaises(ValueError):
                completion.SpaceAgentCompletionConfig.from_environment()


class SpaceAgentCompletionProtocolTests(unittest.TestCase):
    def test_request_authentication_strips_bearer_from_model_input(self):
        validated = completion._validate_request(request_payload(), TOKEN_SHA256)
        self.assertNotIn("runner_token", validated)
        self.assertNotIn(TOKEN, json.dumps(validated))
        self.assertEqual(validated["capabilities"][0]["capability_key"], CAPABILITY_KEY)

        wrong = request_payload()
        wrong["runner_token"] = "wrong-runner-token-with-at-least-thirty-two-bytes"
        with self.assertRaises(ValueError):
            completion._validate_request(wrong, TOKEN_SHA256)

    def test_request_rejects_unknown_fields_duplicate_call_ids_and_long_history(self):
        forged = request_payload()
        forged["space_id"] = str(uuid.uuid4())
        with self.assertRaises(ValueError):
            completion._validate_request(forged, TOKEN_SHA256)

        call_id = str(uuid.uuid4())
        item = {
            "call_id": call_id,
            "capability_key": CAPABILITY_KEY,
            "input": {},
            "output": {"members": []},
        }
        with self.assertRaises(ValueError):
            completion._validate_request(request_payload(history=[item, item]), TOKEN_SHA256)
        with self.assertRaises(ValueError):
            completion._validate_request(
                request_payload(history=[{**item, "call_id": str(uuid.uuid4())} for _ in range(17)]),
                TOKEN_SHA256,
            )

    def test_strict_json_rejects_duplicates_non_finite_numbers_and_excess_depth(self):
        with self.assertRaises(ValueError):
            completion._strict_json_object(b'{"schema_version":1,"schema_version":2}')
        with self.assertRaises(ValueError):
            completion._strict_json_object(b'{"value":NaN}')
        with self.assertRaises(ValueError):
            completion._strict_json_object(b'{"value":1e999}')
        value: dict = {"leaf": True}
        for _ in range(completion.MAX_JSON_DEPTH + 1):
            value = {"child": value}
        with self.assertRaises(ValueError):
            completion._strict_json_object(json.dumps(value).encode("utf-8"))

    def test_decision_is_only_final_or_new_catalog_capability_call(self):
        prompt = completion._validate_request(request_payload(), TOKEN_SHA256)
        final = {"kind": "final", "output": {"summary": "done"}}
        self.assertEqual(completion._validate_decision(final, prompt), final)

        call = {
            "kind": "capability_call",
            "call_id": str(uuid.uuid4()),
            "capability_key": CAPABILITY_KEY,
            "input": {},
        }
        self.assertEqual(completion._validate_decision(call, prompt), call)
        for forged in [
            {**call, "capability_key": "erhua.qiwe_send_direct_message"},
            {**call, "target": "forged-room"},
            {"kind": "other", "output": {}},
        ]:
            with self.assertRaises(ValueError):
                completion._validate_decision(forged, prompt)

        history_item = {
            "call_id": call["call_id"],
            "capability_key": CAPABILITY_KEY,
            "input": {},
            "output": {"members": []},
        }
        completed_prompt = completion._validate_request(
            request_payload(history=[history_item]), TOKEN_SHA256
        )
        with self.assertRaises(ValueError):
            completion._validate_decision(call, completed_prompt)

    @unittest.skipUnless(hasattr(completion.socket, "SO_PEERCRED"), "Linux peer credentials")
    def test_peer_uid_and_gid_must_both_match(self):
        writer = FakeWriter(FakePeerSocket(123, 2001, 2002))
        completion._validate_peer(writer, 2001, 2002)
        with self.assertRaises(ValueError):
            completion._validate_peer(writer, 2003, 2002)
        with self.assertRaises(ValueError):
            completion._validate_peer(writer, 2001, 2004)


class SpaceAgentCompletionServerTests(unittest.IsolatedAsyncioTestCase):
    async def test_disabled_server_needs_no_model_or_socket(self):
        server = completion.SpaceAgentCompletionServer(
            None, completion.SpaceAgentCompletionConfig(enabled=False)
        )
        await server.start()
        await server.stop()

    async def test_enabled_server_requires_hermes_llm(self):
        with tempfile.TemporaryDirectory() as directory:
            config = completion.SpaceAgentCompletionConfig(
                enabled=True,
                socket_path=Path(directory) / "completion.sock",
                token_sha256=TOKEN_SHA256,
                runner_uid=max(os.geteuid(), 1),
                runner_gid=max(os.getegid(), 1),
            )
            server = completion.SpaceAgentCompletionServer(None, config)
            with self.assertRaises(RuntimeError):
                await server.start()

    async def test_lifecycle_uses_only_configured_unix_socket(self):
        llm = FakeLlm({"kind": "final", "output": {"summary": "bounded"}})
        with tempfile.TemporaryDirectory() as directory:
            Path(directory).chmod(0o750)
            socket_path = Path(directory) / "completion.sock"
            config = completion.SpaceAgentCompletionConfig(
                enabled=True,
                socket_path=socket_path,
                token_sha256=TOKEN_SHA256,
                runner_uid=max(os.geteuid() + 1, 1),
                runner_gid=max(os.getegid(), 1),
            )
            server = completion.SpaceAgentCompletionServer(llm, config)
            fake_server = FakeAsyncServer()
            with (
                patch.object(
                    completion.asyncio,
                    "start_unix_server",
                    new=AsyncMock(return_value=fake_server),
                ) as start_unix_server,
                patch.object(completion.os, "chmod") as chmod,
                patch.object(completion.os, "chown") as chown,
            ):
                await server.start()
                await server.stop()
        start_unix_server.assert_awaited_once()
        self.assertEqual(start_unix_server.await_args.kwargs["path"], str(socket_path))
        chmod.assert_called_once_with(socket_path, 0o660)
        chown.assert_called_once_with(socket_path, -1, config.runner_gid)
        self.assertTrue(fake_server.closed)

    async def test_lifecycle_rejects_runner_owned_or_group_writable_parent(self):
        llm = FakeLlm({"kind": "final", "output": {"summary": "bounded"}})
        with tempfile.TemporaryDirectory() as directory:
            socket_path = Path(directory) / "completion.sock"
            config = completion.SpaceAgentCompletionConfig(
                enabled=True,
                socket_path=socket_path,
                token_sha256=TOKEN_SHA256,
                runner_uid=max(os.geteuid(), 1),
                runner_gid=max(os.getegid(), 1),
            )
            server = completion.SpaceAgentCompletionServer(llm, config)
            with self.assertRaises(ValueError):
                await server.start()

            Path(directory).chmod(0o770)
            config = completion.SpaceAgentCompletionConfig(
                enabled=True,
                socket_path=socket_path,
                token_sha256=TOKEN_SHA256,
                runner_uid=max(os.geteuid() + 1, 1),
                runner_gid=max(os.getegid(), 1),
            )
            server = completion.SpaceAgentCompletionServer(llm, config)
            with self.assertRaises(ValueError):
                await server.start()

    async def test_handler_returns_exact_bounded_decision_and_never_prompts_with_bearer(self):
        decision = {"kind": "final", "output": {"summary": "bounded"}}
        llm = FakeLlm(decision)
        config = completion.SpaceAgentCompletionConfig(
            enabled=True,
            token_sha256=TOKEN_SHA256,
            runner_uid=max(os.geteuid(), 1),
            runner_gid=max(os.getegid(), 1),
            timeout_seconds=7,
        )
        server = completion.SpaceAgentCompletionServer(llm, config)
        reader = AsyncMock()
        reader.readline.return_value = json.dumps(request_payload()).encode("utf-8") + b"\n"
        writer = RecordingConnectionWriter()
        with patch.object(completion, "_validate_peer", return_value=None):
            await server._handle_connection(reader, writer)
        response = json.loads(writer.output.decode("utf-8"))
        self.assertEqual(
            response,
            {"schema_version": 1, "accepted": True, "decision": decision},
        )
        self.assertEqual(len(llm.calls), 1)
        call = llm.calls[0]
        self.assertEqual(call["purpose"], "qintopia_space_agent_turn")
        self.assertEqual(call["timeout"], 7)
        self.assertNotIn(TOKEN, call["messages"][1]["content"])
        self.assertTrue(writer.closed)

    async def test_invalid_model_decision_returns_only_rejected_envelope(self):
        llm = FakeLlm({"kind": "capability_call", "capability_key": CAPABILITY_KEY, "input": {}})
        config = completion.SpaceAgentCompletionConfig(
            enabled=True,
            token_sha256=TOKEN_SHA256,
            runner_uid=max(os.geteuid(), 1),
            runner_gid=max(os.getegid(), 1),
        )
        server = completion.SpaceAgentCompletionServer(llm, config)
        reader = AsyncMock()
        reader.readline.return_value = json.dumps(request_payload()).encode("utf-8") + b"\n"
        writer = RecordingConnectionWriter()
        with patch.object(completion, "_validate_peer", return_value=None):
            await server._handle_connection(reader, writer)
        response = json.loads(writer.output.decode("utf-8"))
        self.assertEqual(
            response,
            {"schema_version": 1, "accepted": False, "decision": None},
        )


if __name__ == "__main__":
    unittest.main()
