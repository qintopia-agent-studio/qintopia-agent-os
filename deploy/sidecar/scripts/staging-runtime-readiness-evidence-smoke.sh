#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE:-}" != "1" ]]; then
  echo "staging runtime readiness evidence skipped: set QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 to collect sanitized staging runtime evidence" >&2
  exit 0
fi

TEST_MODE="${QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_TEST_MODE:-0}"
if [[ "$TEST_MODE" != "0" && "$TEST_MODE" != "1" ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_TEST_MODE must be 0 or 1" >&2
  exit 1
fi

RELEASE_SHA="${QINTOPIA_STAGING_RUNTIME_RELEASE_SHA:-}"
SIDECAR_SHA256="${QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256:-}"
DATABASE_URL_SHA256="${QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256:-}"

if [[ ! "$RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_RELEASE_SHA must be a 40-character lowercase hex SHA" >&2
  exit 1
fi

if [[ ! "$SIDECAR_SHA256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256 must be a canonical SHA-256" >&2
  exit 1
fi

if [[ ! "$DATABASE_URL_SHA256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256 must be a canonical SHA-256" >&2
  exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

STAGING_RUNTIME_SCRIPT_DIR="$SCRIPT_DIR" \
STAGING_RUNTIME_TEST_MODE="$TEST_MODE" \
STAGING_RUNTIME_RELEASE_SHA="$RELEASE_SHA" \
STAGING_RUNTIME_SIDECAR_SHA256="$SIDECAR_SHA256" \
STAGING_RUNTIME_DATABASE_URL_SHA256="$DATABASE_URL_SHA256" \
STAGING_RUNTIME_ENV_FILE="${QINTOPIA_STAGING_RUNTIME_READINESS_ENV_FILE:-}" \
STAGING_RUNTIME_RELEASE_ROOT="${QINTOPIA_STAGING_RUNTIME_READINESS_RELEASE_ROOT:-}" \
python3 - <<'PY'
import json
import os
import subprocess
import sys


script_dir = os.environ["STAGING_RUNTIME_SCRIPT_DIR"]
test_mode = os.environ["STAGING_RUNTIME_TEST_MODE"]
release_sha = os.environ["STAGING_RUNTIME_RELEASE_SHA"]
sidecar_sha256 = os.environ["STAGING_RUNTIME_SIDECAR_SHA256"]
database_url_sha256 = os.environ["STAGING_RUNTIME_DATABASE_URL_SHA256"]
env_file = os.environ.get("STAGING_RUNTIME_ENV_FILE", "")
release_root = os.environ.get("STAGING_RUNTIME_RELEASE_ROOT", "")


def minimal_env(extra):
    env = {
        "PATH": os.environ.get("PATH", "/usr/local/bin:/usr/bin:/bin"),
    }
    env.update(extra)
    if test_mode == "1":
        if env_file:
            env.update(extra_env_file(env_file))
        if release_root:
            env.update(extra_release_root(release_root))
    return env


def extra_env_file(path):
    return {
        "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENV_FILE": path,
        "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENV_FILE": path,
        "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENV_FILE": path,
    }


def extra_release_root(path):
    return {
        "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_RELEASE_ROOT": path,
        "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_RELEASE_ROOT": path,
        "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_RELEASE_ROOT": path,
    }


def run_report(label, script_name, prefix, env):
    result = subprocess.run(
        ["bash", os.path.join(script_dir, script_name)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        env=env,
        check=False,
    )
    line = next(
        (entry for entry in result.stdout.splitlines() if entry.startswith(prefix)),
        None,
    )
    if line is None:
        return {
            "label": label,
            "success": False,
            "action_status": "readiness_report_missing",
            "exit_code": result.returncode,
            "limitations": [f"{label}_report_missing"],
        }
    try:
        report = json.loads(line[len(prefix) :])
    except json.JSONDecodeError:
        return {
            "label": label,
            "success": False,
            "action_status": "readiness_report_invalid_json",
            "exit_code": result.returncode,
            "limitations": [f"{label}_report_invalid_json"],
        }
    ready = report_is_ready(label, report, result.returncode)
    return {
        "label": label,
        "success": ready,
        "action_status": report.get("action_status"),
        "exit_code": result.returncode,
        "env_file_present": bool(report.get("env_file_present")),
        "env_file_secure": bool(report.get("env_file_secure")),
        "release_root_present": bool(report.get("release_root_present")),
        "release_root_secure": bool(report.get("release_root_secure")),
        "sidecar_binary_present": bool(report.get("sidecar_binary_present")),
        "sidecar_binary_secure": bool(report.get("sidecar_binary_secure")),
        "sidecar_hash_matches": bool(report.get("sidecar_hash_matches")),
        "sidecar_binary_sha256": report.get("sidecar_binary_sha256"),
        "limitations": list(report.get("limitations") or []),
    }


def report_is_ready(label, report, exit_code):
    if exit_code != 0:
        return False
    action_status = report.get("action_status")
    if label == "prerequisite":
        return (
            report.get("ready_for_staging") is True
            and action_status == "ready_for_staging_readiness_smokes"
        )
    return (
        report.get("success") is True
        and action_status == "ready_for_staging_preflight"
    )


base_test = {
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE": test_mode,
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_TEST_MODE": test_mode,
    "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_TEST_MODE": test_mode,
}

reports = [
    run_report(
        "prerequisite",
        "staging-runtime-prerequisite-observation-smoke.sh",
        "staging_runtime_prerequisite_observation=",
        minimal_env(
            {
                **base_test,
                "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE": "1",
                "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA": release_sha,
                "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256": sidecar_sha256,
            }
        ),
    ),
    run_report(
        "huabaosi_readiness",
        "huabaosi-image-generation-staging-readiness-smoke.sh",
        "huabaosi_image_generation_staging_readiness=",
        minimal_env(
            {
                **base_test,
                "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE": "1",
                "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL": "approved-staging-image-generation",
                "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA": release_sha,
                "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256": sidecar_sha256,
            }
        ),
    ),
    run_report(
        "qiwe_readiness",
        "qiwe-image-send-staging-readiness-smoke.sh",
        "qiwe_image_send_staging_readiness=",
        minimal_env(
            {
                **base_test,
                "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE": "1",
                "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL": "approved-staging-qiwe-image-send",
                "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA": release_sha,
                "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256": sidecar_sha256,
            }
        ),
    ),
]

limitations = []
for report in reports:
    for limitation in report["limitations"]:
        value = f'{report["label"]}_{limitation}'
        if value not in limitations:
            limitations.append(value)

all_ready = all(report["success"] for report in reports)
evidence = {
    "success": all_ready,
    "worker": "staging-runtime-readiness-evidence",
    "action_status": "ready_for_huabaosi_qiwe_staging_smokes" if all_ready else "not_ready",
    "test_mode": test_mode == "1",
    "release_sha": release_sha,
    "packaged_sidecar_sha256": sidecar_sha256,
    "staging_database_url_sha256": database_url_sha256,
    "reports": reports,
    "safe_for_review": True,
    "limitations": limitations,
    "guardrails": [
        "read-only readiness evidence aggregation",
        "staging env file contents are not read",
        "child readiness scripts run with a minimal explicit environment",
        "sidecar binary is not executed",
        "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    ],
}

print(
    "staging_runtime_readiness_evidence="
    + json.dumps(evidence, sort_keys=True, separators=(",", ":"))
)
sys.exit(0 if all_ready else 1)
PY
