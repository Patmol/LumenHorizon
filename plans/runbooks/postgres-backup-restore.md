# PostgreSQL Backup And Restore Practice

Use this runbook to practice local database backup and restore behavior.

## Backup

1. Start local dependencies with `just up`.
2. Confirm `.env` contains `DATABASE_URL`.
3. Create a dump with `pg_dump` using the local connection settings.
4. Store the dump outside the repository or in an ignored scratch location.

## Restore Practice

1. Stop services that may write to the database.
2. Create a fresh local database or reset the local container volume.
3. Restore the dump with `psql` or `pg_restore`, depending on dump format.
4. Run `just migrate` to confirm migrations are consistent.
5. Run focused smoke checks for ingest run lists, processing run lists, and tile-set metadata.

## Evidence To Record

- Dump command and timestamp.
- Restore command and target database.
- Migration result.
- Any smoke queries or API calls used to confirm restored data.

Do not store dumps containing real credentials or sensitive data in the repository.
