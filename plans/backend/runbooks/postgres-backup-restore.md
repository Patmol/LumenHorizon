# PostgreSQL Backup And Restore Practice

Use this runbook to practice local database backup and restore behavior. It is intended for local PostgreSQL only.

## Prerequisites

```bash
just setup
just up
just migrate
set -a && source .env && set +a
```

Expected result:

- `.env` exists and contains `DATABASE_URL`.
- `just up` starts the local `lumenhorizon-postgres` dependency.
- `just migrate` reports the migrations applied or already current.

## Backup

Create a timestamped custom-format dump in an ignored scratch directory:

```bash
mkdir -p tmp/backups
pg_dump "$DATABASE_URL" --format=custom --file "tmp/backups/lumenhorizon-$(date +%Y%m%dT%H%M%S).dump"
```

Confirm the dump exists:

```bash
ls -lh tmp/backups/*.dump
```

Expected result: the dump file is non-empty and owned by the current user.

## Restore Practice

Stop services that may write to the database before restoring. For a destructive local restore practice, use a scratch database name so the main local database stays available:

```bash
RESTORE_DATABASE_URL="${DATABASE_URL%/*}/lumenhorizon_restore_check"
psql "$DATABASE_URL" -c "drop database if exists lumenhorizon_restore_check;"
psql "$DATABASE_URL" -c "create database lumenhorizon_restore_check;"
pg_restore --dbname "$RESTORE_DATABASE_URL" --clean --if-exists tmp/backups/<dump-file>.dump
```

Run schema and smoke checks against the restored database:

```bash
psql "$RESTORE_DATABASE_URL" -c "select count(*) as ingest_rows from ingest_log;"
psql "$RESTORE_DATABASE_URL" -c "select count(*) as processing_rows from processing_log;"
psql "$RESTORE_DATABASE_URL" -c "select count(*) as tile_sets from tile_sets;"
```

Expected result: all queries complete successfully. Counts may be zero for a fresh local database.

Drop the scratch database when the practice run is complete:

```bash
psql "$DATABASE_URL" -c "drop database if exists lumenhorizon_restore_check;"
```

## Evidence To Record

- Dump command, timestamp, and dump file size.
- Restore command and target database.
- Migration result from `just migrate` before the backup.
- Smoke query results for `ingest_log`, `processing_log`, and `tile_sets`.

Do not store dumps containing real credentials or sensitive data in the repository.