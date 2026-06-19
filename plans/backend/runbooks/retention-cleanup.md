# Retention Cleanup Runbook

Use this runbook to preview or execute local cleanup of stale raw blobs and processed tile sets.

## Prerequisites

```bash
just setup
just up
just migrate
set -a && source .env && set +a
```

Expected result:

- `RAW_GRANULE_RETENTION_DAYS`, `PROCESSED_TILE_SET_RETENTION_DAYS`, `RETENTION_PROTECTED_PRIOR_TILE_SETS`, `RETENTION_BATCH_LIMIT`, and `RETENTION_TILE_BLOB_LIMIT` are set from `.env`.
- PostgreSQL and Azurite are running locally.

## Dry Run

Dry run is the default and should be used before every execute run:

```bash
just retention-cleanup
```

Expected result: the command reports selected raw blobs and tile sets, then completes without deleting blobs. The final summary includes selected counts and zero execute deletions.

Review selected cleanup events:

```bash
psql "$DATABASE_URL" -c "select target_kind, action, mode, reason, created_at from retention_cleanup_events order by created_at desc limit 20;"
```

Expected result: recent dry-run records use `mode = 'dry_run'` and `action = 'selected'` or `action = 'skipped'`.

## Execute

Run execute mode only after reviewing dry-run output:

```bash
just retention-cleanup-execute
```

Expected result: eligible blobs are deleted or recorded as already missing, skipped tile sets are left untouched, and cleanup audit events are written.

Verify execute records:

```bash
psql "$DATABASE_URL" -c "select target_kind, action, mode, count(*) from retention_cleanup_events group by target_kind, action, mode order by target_kind, action, mode;"
psql "$DATABASE_URL" -c "select id, classification_version, latest, retention_deleted_at, retention_delete_reason from tile_sets order by created_at desc limit 20;"
```

Expected result:

- Execute records use `mode = 'execute'`.
- Latest tile sets keep `retention_deleted_at` unset.
- Deleted stale tile sets have `retention_deleted_at` and `retention_delete_reason` populated.

## Safety Rules

- Latest plus two prior tile sets per classification version are protected by the default `.env.example` policy.
- Tile sets that cannot be safely listed within `RETENTION_TILE_BLOB_LIMIT` are skipped.
- `manifests/latest.json` is preserved.
- Cleanup must be idempotent; rerunning should not fail because an already-selected blob is gone.

## Evidence To Record

- Dry-run command output.
- Execute command output, if run.
- Cleanup audit rows from `retention_cleanup_events`.
- `tile_sets` rows showing protected latest tile sets and deleted stale tile sets.
- Blob listing spot checks for protected and deleted prefixes, when using an Azurite-compatible storage browser.