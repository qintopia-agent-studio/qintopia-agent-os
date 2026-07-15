#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:-}" != "approved-production-huabaosi-feishu-artifact-mirror" ]]; then
  echo "Huabaosi Feishu mirror production activation requires explicit owner approval" >&2
  exit 1
fi

echo "Huabaosi Feishu mirror production activation is disabled until a separate owner-reviewed release boundary adds the mirror adapter artifact and timer" >&2
exit 1
