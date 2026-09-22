"""Actual registered local plugins and one-shot host protocol; no real LLM/channel."""
import base64
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from uuid import uuid4

from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
RUNNER = ROOT / "workflows/resident-welcome/scripts/local_agent_runtime.py"
CASE = "11111111-1111-4111-8111-111111111111"
TARGET = "22222222-2222-4222-8222-222222222222"
ARTIFACT = "33333333-3333-4333-8333-333333333333"
REVIEWER = "44444444-4444-4444-8444-444444444444"
ACTION = "55555555-5555-4555-8555-555555555555"
ATTEMPT = "66666666-6666-4666-8666-666666666666"
CONTENT_HASH = "a" * 64


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def envelope(agent, operation, value):
    return {"schema_version": 1, "call_id": str(uuid4()), "agent": agent, "operation": operation,
            "trusted_context": {"task_ref": str(uuid4()), "target_agent": agent,
                                "capability_key": "resident_welcome.coordinate", "source_type": "resident_welcome", "input": value},
            "arguments": {}}


class LocalAgentRuntimeTests(unittest.TestCase):
    def run_request(self, request, expected_success=True):
        raw = request if isinstance(request, bytes) else encoded(request)
        process = subprocess.run([sys.executable, str(RUNNER)], input=raw, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, timeout=15, check=False)
        self.assertEqual(process.stderr, b"")
        result = json.loads(process.stdout)
        self.assertEqual(process.returncode, 0 if expected_success else 2, result)
        if expected_success:
            self.assertEqual(result["call_id"], request["call_id"])
            self.assertEqual(result["task_ref"], request["trusted_context"]["task_ref"])
            self.assertEqual(result["agent"], request["agent"])
            self.assertEqual(result["operation"], request["operation"])
            self.assertEqual(result["runtime"], "local_scripted_agent_runtime")
            self.assertEqual(result["tool_name"], "qintopia_welcome_" + request["operation"])
            self.assertEqual(result["plugin_id"], "qintopia-welcome-" + request["agent"])
            self.assertEqual(result["request_sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(result["output_sha256"], hashlib.sha256(encoded(result["output"])).hexdigest())
            plugin = ROOT / "agents" / request["agent"] / "welcome_runtime.py"
            self.assertEqual(result["plugin_source_sha256"], hashlib.sha256(plugin.read_bytes()).hexdigest())
        else:
            self.assertEqual(result["ok"], False)
        return result

    def test_anan_registered_tools_request_card_and_plan_actual_parts(self):
        request = envelope("anan", "request_card", {"case_ref": CASE, "case_version": 7})
        self.assertEqual(self.run_request(request)["output"], {
            "command": "request_card", "target_agent": "huabaosi", "case_ref": CASE, "case_version": 7})
        request = envelope("anan", "prepare", {"case_ref": CASE, "target_ref": TARGET, "phase": "formal",
            "setting": {"mode": "direct", "parts": ["text"], "phase": "formal", "text_template": "欢迎 {name}"}})
        expected = {"command": "prepare_parts", "case_ref": CASE, "target_ref": TARGET, "phase": "formal",
                    "parts": [{"part": "image", "requires_content_review": True},
                              {"part": "text", "requires_content_review": False}]}
        self.assertEqual(self.run_request(request)["output"], expected)
        request["trusted_context"]["input"]["setting"]["mode"] = "review"
        expected["parts"][1]["requires_content_review"] = True
        self.assertEqual(self.run_request(request)["output"], expected)

    def test_huabaosi_registered_tool_returns_real_png_and_content_hash(self):
        request = envelope("huabaosi", "render_card", {"case_ref": CASE, "case_version": 3,
            "material": {"display_name": "合成小林", "description": "喜欢阅读与散步。"}})
        output = self.run_request(request)["output"]
        self.assertEqual(output["command"], "register_card")
        self.assertEqual(output["media_type"], "image/png")
        self.assertEqual((output["case_ref"], output["case_version"]), (CASE, 3))
        content = base64.b64decode(output["content_base64"], validate=True)
        self.assertEqual(output["content_hash"], hashlib.sha256(content).hexdigest())
        with Image.open(io.BytesIO(content)) as image:
            image.load()
            self.assertEqual(image.size, (1080, 720))
            self.assertGreater(len(image.getcolors(maxcolors=1080 * 720)), 20)

    def test_erhua_registered_tools_preserve_exact_review_and_send_binding(self):
        review = {"case_ref": CASE, "target_ref": TARGET, "phase": "formal", "artifact_ref": ARTIFACT,
                  "content_hash": CONTENT_HASH, "approval_kind": "publish_confirmation", "reviewer": REVIEWER}
        self.assertEqual(self.run_request(envelope("erhua", "forward_review", review))["output"],
                         {"command": "forward_review", **review})
        forward = {"action_ref": ACTION, "artifact_ref": ARTIFACT, "content_hash": CONTENT_HASH,
                   "target_ref": TARGET, "phase": "formal", "part": "image", "attempt_ref": ATTEMPT}
        self.assertEqual(self.run_request(envelope("erhua", "forward", forward))["output"],
                         {"command": "forward", **forward})

    def test_wrong_agent_task_context_and_unknown_tool_are_rejected(self):
        valid = envelope("anan", "request_card", {"case_ref": CASE, "case_version": 0})
        variants = []
        for key, value in [("target_agent", "erhua"), ("capability_key", "other.capability"),
                           ("source_type", "chat"), ("task_ref", "not-a-task")]:
            request = copy.deepcopy(valid)
            request["trusted_context"][key] = value
            variants.append(request)
        for key, value in [("agent", "silaoshi"), ("operation", "render_card"),
                           ("operation", "arbitrary_tool"), ("schema_version", True)]:
            request = copy.deepcopy(valid)
            request[key] = value
            variants.append(request)
        for request in variants:
            with self.subTest(request=request):
                self.run_request(request, False)

    def test_model_identity_or_destinations_and_extra_host_fields_are_rejected(self):
        valid = envelope("anan", "request_card", {"case_ref": CASE, "case_version": 0})
        for field, value in [("actor", REVIEWER), ("task_ref", ACTION), ("destination", "https://example.test"),
                             ("case_ref", TARGET)]:
            request = copy.deepcopy(valid)
            request["arguments"][field] = value
            self.assertEqual(self.run_request(request, False)["error"]["code"], "invalid_fields")
        request = copy.deepcopy(valid)
        request["trusted_context"]["input"]["shell"] = "unexpected command"
        self.run_request(request, False)
        request = envelope("huabaosi", "render_card", {"case_ref": CASE, "case_version": 0,
            "material": {"display_name": "合成", "description": "", "photo_url": "https://example.test"}})
        self.run_request(request, False)

    def test_duplicate_fields_oversize_and_invalid_artifact_hash_fail_closed(self):
        request = encoded(envelope("anan", "request_card", {"case_ref": CASE, "case_version": 0}))
        request = request.replace(b'"case_version":0', b'"case_version":0,"case_version":1')
        self.assertEqual(self.run_request(request, False)["error"]["code"], "duplicate_field")
        self.assertEqual(self.run_request(b" " * 65537, False)["error"]["code"], "input_too_large")
        self.run_request(envelope("erhua", "forward", {"action_ref": ACTION, "artifact_ref": ARTIFACT,
            "content_hash": "invalid", "target_ref": TARGET, "phase": "formal", "part": "image", "attempt_ref": ATTEMPT}), False)

    def test_process_host_drops_secrets_and_denies_network_and_subprocesses(self):
        # Inspect the host boundary in an isolated process; no real plugin is replaced
        # in the integration tests above, and no secret value is printed.
        script = '''
import json, os, pathlib, socket, subprocess, sys
sys.path.insert(0, sys.argv[1])
import local_agent_runtime as runtime
def inspect(_):
    assert "QINTOPIA_SIDECAR_DATABASE_URL" not in os.environ
    assert "QIWE_TOKEN" not in os.environ
    assert "PYTHONPATH" not in os.environ
    denied = 0
    for operation in [lambda: socket.socket(), lambda: subprocess.run([sys.executable, "-V"])]:
        try:
            operation()
        except PermissionError:
            denied += 1
    assert denied == 2
    return {"environment_filtered": True, "external_effects_denied": denied}
runtime.execute = inspect
raise SystemExit(runtime.main())
'''
        env = dict(os.environ, QINTOPIA_SIDECAR_DATABASE_URL="synthetic-private-value", QIWE_TOKEN="synthetic-token", PYTHONPATH="/not-a-real-python-path")
        process = subprocess.run([sys.executable, "-c", script, str(RUNNER.parent)], input=b"{}",
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, timeout=10)
        self.assertEqual(process.returncode, 0, process.stderr.decode())
        self.assertEqual(json.loads(process.stdout), {"environment_filtered": True, "external_effects_denied": 2})
        self.assertNotIn(b"synthetic-private-value", process.stdout + process.stderr)
        self.assertNotIn(b"synthetic-token", process.stdout + process.stderr)


if __name__ == "__main__":
    unittest.main()
