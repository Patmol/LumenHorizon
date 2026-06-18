#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
. "${ROOT_DIR}/scripts/lib.sh"

load_env

AZURITE_ACCOUNT="devstoreaccount1"
AZURITE_ACCOUNT_KEY="Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw=="
AZURITE_PROTOCOL="http"
AZURITE_HOST="${AZURE_STORAGE_EMULATOR_HOST:-127.0.0.1}"
AZURITE_CONNECTION_STRING="DefaultEndpointsProtocol=${AZURITE_PROTOCOL};"
AZURITE_CONNECTION_STRING+="AccountName=${AZURITE_ACCOUNT};"
AZURITE_CONNECTION_STRING+="AccountKey=${AZURITE_ACCOUNT_KEY};"
AZURITE_CONNECTION_STRING+="BlobEndpoint=${AZURITE_PROTOCOL}://${AZURITE_HOST}:10000/${AZURITE_ACCOUNT};"
AZURITE_CONNECTION_STRING+="QueueEndpoint=${AZURITE_PROTOCOL}://${AZURITE_HOST}:10001/${AZURITE_ACCOUNT};"
AZURITE_CONNECTION_STRING+="TableEndpoint=${AZURITE_PROTOCOL}://${AZURITE_HOST}:10002/${AZURITE_ACCOUNT};"

start_local_dependencies

wait_for_postgres 90

echo "==> Creating Azurite containers and queues..."
az storage container create \
  --name raw-viirs \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --output none
az storage container create \
  --name processed-tiles \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --output none
az storage container create \
  --name user-uploads \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --output none
az storage queue create \
  --name "${AZURE_QUEUE_NAME:-viirs-processing}" \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --output none
az storage queue create \
  --name "${AZURE_DEADLETTER_QUEUE_NAME:-viirs-processing-deadletter}" \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --output none

echo "Local dependencies are ready."
