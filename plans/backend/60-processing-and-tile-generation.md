# 60. Processing And Tile Generation

`processing-svc` turns queued ingest messages into quality-filtered science evidence, generated tiles, manifests, and retention decisions.

## Commands

| Command | Purpose |
| --- | --- |
| `worker` | Drain visible processing queue messages. |
| `process-once` | Process at most one visible message. |
| `process-message <json>` | Process one supplied message payload. |
| `retention-cleanup` | Preview stale raw and tile artifacts. |
| `retention-cleanup --execute` | Delete eligible stale artifacts and record audit rows. |

## Queue Behavior

- Receive visible messages with a visibility timeout.
- Delete messages only after successful processing.
- Retry transient or parsed failures through queue visibility semantics.
- Move exhausted work to the dead-letter queue.
- Use `ingest_id` idempotency to avoid duplicate terminal processing records.

## Science Processing

- Read verified VIIRS Black Marble datasets through the GDAL CLI boundary.
- Use product-specific quality and cloud fields rather than radiance inference alone.
- Persist sampled quality evidence and scalar summary columns.
- Reject high-cloud or incompatible granules with structured reasons.
- Attach versioned `radiance-dark-sky-v1` evidence for accepted products.

## Tile Generation

- Use shared Slippy Map tile math.
- Clip generation to source bounds and configured zoom range.
- Render fixed-size PNG tiles.
- Publish immutable tile manifests and a `manifests/latest.json` pointer.
- Insert `tile_sets` metadata and link processing logs to durable tile output only after publication succeeds.

## Retention Policy

- Raw granules are retained for the configured raw retention window.
- Processed tile sets are retained for the configured tile-set retention window.
- Latest plus two prior tile sets per classification version are protected.
- Cleanup is dry-run by default and requires `--execute` to delete blobs.
- Tile sets that cannot be safely listed within the configured blob limit are skipped.

## Verification

- Tests cover queue receive/delete/dead-letter behavior.
- Tests cover dataset mapping, quality filtering, classification evidence, and rejection reasons.
- Tests cover representative daily and monthly science fixtures, including cloud rejection and monthly observation-count evidence.
- Tests cover tile math, multi-bounds/zoom smoke plans, manifest shape, latest pointer publication, and cleanup selection.
- `just retention-cleanup` and `just retention-cleanup-execute` verify local dry-run and execute retention behavior against stale local metadata and blobs.
