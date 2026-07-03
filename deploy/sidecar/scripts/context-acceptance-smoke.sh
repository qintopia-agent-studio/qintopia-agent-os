#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${QINTOPIA_SIDECAR_REPO_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
BIN="${QINTOPIA_SIDECAR_BIN:-${REPO_DIR}/target/release/qintopia-message-sidecar}"

if [[ ! -x "$BIN" ]]; then
  echo "missing executable: $BIN" >&2
  exit 2
fi

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

if [[ -z "${QINTOPIA_SIDECAR_DATABASE_URL:-}" ]]; then
  echo "QINTOPIA_SIDECAR_DATABASE_URL is required" >&2
  exit 2
fi

python3 - "$BIN" <<'PY'
import json
import subprocess
import sys

bin_path = sys.argv[1]

cases = [
    {
        "id": 2,
        "query": "WiFi 密码是什么",
        "purpose": "acceptance smoke",
        "expected_kind": "authoritative_knowledge",
        "expected_can_answer": True,
        "min_sources": 1,
    },
    {
        "id": 3,
        "query": "赵姐订餐电话是多少",
        "purpose": "acceptance smoke",
        "expected_kind": "authoritative_knowledge",
        "expected_can_answer": True,
        "min_sources": 1,
    },
    {
        "id": 4,
        "query": "无人机外卖怎么用",
        "purpose": "acceptance smoke",
        "expected_kind": "authoritative_knowledge",
        "expected_can_answer": True,
        "min_sources": 1,
    },
    {
        "id": 5,
        "query": "之前群里有人问过 WiFi 密码吗",
        "purpose": "community memory",
        "expected_kind": "message_store_evidence",
        "expected_can_answer": True,
        "min_sources": 1,
    },
    {
        "id": 6,
        "query": "还有空房吗",
        "purpose": "realtime operations smoke",
        "expected_kind": "live_operations_required",
        "expected_can_answer": False,
        "min_sources": 0,
    },
]

messages = [
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "context-acceptance-smoke", "version": "0.1.0"},
        },
    },
    {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
]
for case in cases:
    messages.append(
        {
            "jsonrpc": "2.0",
            "id": case["id"],
            "method": "tools/call",
            "params": {
                "name": "qintopia_wenyuange_lookup",
                "arguments": {
                    "caller": "erhua",
                    "purpose": case["purpose"],
                    "query": case["query"],
                    "limit": 3,
                },
            },
        }
    )

stdin = "\n".join(json.dumps(message, ensure_ascii=False) for message in messages) + "\n"
proc = subprocess.run(
    [bin_path, "mcp-context"],
    input=stdin,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    check=False,
)
if proc.returncode != 0:
    sys.stderr.write(proc.stderr)
    raise SystemExit(proc.returncode)

responses = {}
for line in proc.stdout.splitlines():
    if not line.strip():
        continue
    payload = json.loads(line)
    if "id" in payload:
        responses[payload["id"]] = payload

failures = []
for case in cases:
    response = responses.get(case["id"])
    if not response:
        failures.append(f"{case['query']}: missing response")
        continue
    if "error" in response:
        failures.append(f"{case['query']}: jsonrpc error {response['error']}")
        continue
    content = (response.get("result", {}).get("content") or [{}])[0].get("text", "")
    try:
        data = json.loads(content)
    except Exception as exc:
        failures.append(f"{case['query']}: invalid JSON content: {exc}")
        continue
    kind = (data.get("answer_basis") or {}).get("kind")
    can_answer = data.get("can_answer")
    sources = data.get("sources") or []
    if kind != case["expected_kind"]:
        failures.append(f"{case['query']}: kind={kind}, expected {case['expected_kind']}")
    if can_answer is not case["expected_can_answer"]:
        failures.append(
            f"{case['query']}: can_answer={can_answer}, expected {case['expected_can_answer']}"
        )
    if len(sources) < case["min_sources"]:
        failures.append(
            f"{case['query']}: source_count={len(sources)}, expected >= {case['min_sources']}"
        )
    if case["expected_kind"] == "authoritative_knowledge":
        bad_sources = [source for source in sources if source.get("source_type") != "qintopia_knowledge"]
        if bad_sources:
            failures.append(f"{case['query']}: authoritative answer used non-knowledge sources")
    print(
        json.dumps(
            {
                "query": case["query"],
                "can_answer": can_answer,
                "kind": kind,
                "source_count": len(sources),
                "source_titles": [source.get("title") for source in sources[:3]],
            },
            ensure_ascii=False,
        )
    )

if failures:
    print("context acceptance smoke failed:", file=sys.stderr)
    for failure in failures:
        print(f"- {failure}", file=sys.stderr)
    raise SystemExit(1)
PY
