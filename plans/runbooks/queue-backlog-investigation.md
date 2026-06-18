# Queue Backlog Investigation

Use this runbook when local processing work is not draining as expected.

## Quick Checks

1. Confirm dependencies are running: `just up`.
2. Confirm ingest emitted queue work and rows reached an enqueue-ready or enqueued status.
3. Inspect the processing queue and dead-letter queue with your preferred Azurite-compatible tool.
4. Run one processing pass: `cd backend && set -a && source ../.env && set +a && cargo run -p processing-svc -- process-once`.
5. Inspect `processing_log` for started, processed, retry, or rejected records.

## Common Causes

- Missing or invalid storage configuration.
- Message payload shape does not match the processing contract.
- Source raw blob is missing or has an unexpected path.
- GDAL cannot read the expected HDF-EOS5 datasets.
- Quality filtering rejects the granule.
- Message exceeded max dequeue count and moved to dead-letter.

## Evidence To Record

- Queue length and sample message metadata.
- Relevant `ingest_runs`, ingest row, and `processing_log` records.
- Processing command output.
- Dead-letter message payload and reason when available.
