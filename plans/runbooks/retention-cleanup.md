# Retention Cleanup Runbook

Use this runbook to preview or execute local cleanup of stale raw blobs and processed tile sets.

## Dry Run

Dry run is the default and should be used before every execute run:

```bash
just retention-cleanup
```

Review selected raw blobs, tile sets, skip reasons, protected latest/prior tile sets, and estimated delete counts.

## Execute

Run execute mode only after reviewing dry-run output:

```bash
just retention-cleanup-execute
```

Execution deletes eligible blobs and records cleanup audit events.

## Safety Rules

- Latest plus two prior tile sets per classification version are protected.
- Tile sets that cannot be safely listed within `RETENTION_TILE_BLOB_LIMIT` are skipped.
- `manifests/latest.json` is preserved.
- Cleanup must be idempotent; rerunning should not fail because an already-selected blob is gone.

## Evidence To Record

- Dry-run command output.
- Execute command output, if run.
- Cleanup audit rows.
- Blob listing spot checks for protected and deleted prefixes.
