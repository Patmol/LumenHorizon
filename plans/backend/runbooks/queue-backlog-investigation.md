# Queue Backlog Investigation

Use this runbook when local processing work is not draining as expected.

## Prerequisites

```bash
just setup
just up
just migrate
set -a && source .env && set +a
```

Expected result:

- `AZURE_QUEUE_NAME` is set, usually `viirs-processing`.
- `AZURE_DEADLETTER_QUEUE_NAME` is set, usually `viirs-processing-deadletter`.
- PostgreSQL and Azurite are running locally.

## Database State

Check recent ingest and processing rows:

```bash
psql "$DATABASE_URL" -c "select id, product, cadence, status, created_at, updated_at from ingest_log order by created_at desc limit 10;"
psql "$DATABASE_URL" -c "select ingest_id, status, attempts, tile_set_id, error_message, updated_at from processing_log order by updated_at desc limit 10;"
```

Expected result:

- Ingest rows that should be processed are `downloaded`, `queued`, or equivalent enqueue-ready statuses.
- Processing rows move from `processing` to `processed`, `rejected`, `failed`, or `deadlettered`.
- A growing count of `failed` or `deadlettered` rows means the queue worker is receiving messages but terminal handling is failing.

## Queue Worker Smoke

Run one processing pass:

```bash
cd backend
set -a && source ../.env && set +a
cargo run -p processing-svc -- process-once
```

Expected result:

- If no message is available, the command exits cleanly after reporting no queued work.
- If a message is available, the command logs the ingest id and either records a terminal status or leaves the message for visibility-timeout retry.
- Messages whose source tile bounds do not overlap `TILE_BOUNDS` are terminally marked `rejected` and deleted from the active queue.

## Optional Queue Inspection

Use an Azurite-compatible queue tool, such as Azure Storage Explorer or the Azure CLI, to inspect the live queue and dead-letter queue. With Azure CLI installed, the local development connection string can be used like this:

```bash
AZURITE_CONNECTION_STRING="DefaultEndpointsProtocol=http;AccountName=${AZURE_STORAGE_ACCOUNT};AccountKey=${AZURE_STORAGE_ACCESS_KEY};BlobEndpoint=http://${AZURE_STORAGE_EMULATOR_HOST}:10000/${AZURE_STORAGE_ACCOUNT};QueueEndpoint=http://${AZURE_STORAGE_EMULATOR_HOST}:10001/${AZURE_STORAGE_ACCOUNT};"
az storage message peek --queue-name "$AZURE_QUEUE_NAME" --connection-string "$AZURITE_CONNECTION_STRING" --num-messages 5
az storage message peek --queue-name "$AZURE_DEADLETTER_QUEUE_NAME" --connection-string "$AZURITE_CONNECTION_STRING" --num-messages 5
```

Expected result:

- The processing queue should shrink after successful `process-once` runs.
- Dead-letter messages should correspond to malformed payloads, missing source blobs, exhausted dequeue attempts, or terminal validation failures.

## Common Causes

- Missing or invalid storage configuration.
- Message payload shape does not match the processing contract.
- Source raw blob is missing or has an unexpected relative path.
- GDAL cannot read the expected HDF-EOS5 datasets.
- Source tile bounds do not overlap `TILE_BOUNDS`; these messages should become `rejected`, not repeatedly retried.
- Quality filtering rejects the granule.
- Message exceeded max dequeue count and moved to the dead-letter queue.

## Evidence To Record

- Recent `ingest_log` and `processing_log` rows.
- Queue and dead-letter sample message metadata, when inspected.
- `process-once` command output.
- Dead-letter payload and associated `processing_log.error_message`, when available.