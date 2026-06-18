#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "${ROOT_DIR}/scripts/lib.sh"

load_env
wait_for_postgres 60

if [[ "${EARTHDATA_BEARER_TOKEN:-}" == "" || "${EARTHDATA_BEARER_TOKEN:-}" == "replace-me" ]]; then
  echo "Error: EARTHDATA_BEARER_TOKEN must be set to a real token for raw ingest."
  exit 1
fi

cadence="${1:-daily}"

case "${cadence}" in
  daily|monthly)
    ;;
  *)
    echo "Error: ingest cadence must be 'daily' or 'monthly'."
    exit 1
    ;;
esac

echo "==> Running one controlled ${cadence} raw blob ingest..."
(
  cd "${ROOT_DIR}/backend"
  cargo run -p ingest-svc -- ingest "${cadence}"
)
