#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "${ROOT_DIR}/scripts/lib.sh"

load_env

ingest_service_port="${PORT:-8083}"

export PORT="${API_GATEWAY_PORT:-8080}"
export RUST_LOG="${API_GATEWAY_RUST_LOG:-api_gateway=info}"
export RUNTIME_ENVIRONMENT="${RUNTIME_ENVIRONMENT:-local}"
export RATE_LIMIT_BACKEND="${RATE_LIMIT_BACKEND:-memory}"
export JWT_ISSUER="${JWT_ISSUER:-https://login.microsoftonline.com/common/v2.0}"
export JWT_AUDIENCE="${JWT_AUDIENCE:-api://lumenhorizon-admin}"
export JWKS_URL="${JWKS_URL:-https://login.microsoftonline.com/common/discovery/v2.0/keys}"
export INGEST_SERVICE_BASE_URL="${INGEST_SERVICE_BASE_URL:-http://localhost:${ingest_service_port}}"

echo "==> Starting API Gateway on port ${PORT}..."
(
  cd "${ROOT_DIR}/backend"
  cargo run -p api-gateway -- serve
)