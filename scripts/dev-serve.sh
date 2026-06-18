#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "${ROOT_DIR}/scripts/lib.sh"

load_env
wait_for_postgres 60

echo "==> Starting ingest service on port ${PORT:-8083}..."
(
  cd "${ROOT_DIR}/backend"
  cargo run -p ingest-svc -- serve
)
