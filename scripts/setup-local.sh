#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT_DIR}/.env"
EXAMPLE_ENV_FILE="${ROOT_DIR}/.env.example"
. "${ROOT_DIR}/scripts/lib.sh"

require_command cargo
require_command az
CONTAINER_RUNTIME="$(container_runtime)"

if [[ ! -f "$ENV_FILE" ]]; then
  cp "$EXAMPLE_ENV_FILE" "$ENV_FILE"
  echo "Created .env from .env.example. Update EARTHDATA_BEARER_TOKEN before running real ingest."
else
  echo ".env already exists; leaving it unchanged."
fi

echo "Local tool check passed using $(container_runtime_label "${CONTAINER_RUNTIME}")."
