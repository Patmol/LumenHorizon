# Database Schema Reference

PostgreSQL is the shared backend database. Migrations are applied by the dedicated `db-migrate` binary, not by long-running service startup.

## Migration Strategy

`db-migrate` owns cross-service migrations once more than one service has database tables. Service crates may own query code for their own workflows, but schema changes belong in the migration package.

## `ingest_log`

First migration:

```sql
CREATE TABLE IF NOT EXISTS ingest_log (
    id UUID PRIMARY KEY,
    product TEXT NOT NULL,
    granule_title TEXT NOT NULL,
    blob_path TEXT NOT NULL,
    tile_h SMALLINT NOT NULL,
    tile_v SMALLINT NOT NULL,
    granule_date TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ingest_log_status ON ingest_log(status);
CREATE INDEX IF NOT EXISTS idx_ingest_log_granule_date ON ingest_log(granule_date DESC);
CREATE INDEX IF NOT EXISTS idx_ingest_log_product_date ON ingest_log(product, granule_date DESC);
```

Logical granule identity migration:

```sql
ALTER TABLE ingest_log
ADD CONSTRAINT uq_ingest_log_product_tile_date
UNIQUE (product, tile_h, tile_v, granule_date);
```

Allowed statuses: `downloading`, `downloaded`, `validated`, `enqueued`, `rejected`, `failed`.

Chunk 4 adds `recovery_pending` and `replay_pending` to support durable enqueue recovery and explicit rejected replay.

## `processing_log`

Added for Chunk 8.1 processing queue skeleton:

```sql
CREATE TABLE IF NOT EXISTS processing_log (
    id UUID PRIMARY KEY,
    ingest_id UUID NOT NULL REFERENCES ingest_log(id),
    source_blob_path TEXT NOT NULL,
    product TEXT NOT NULL,
    tile_h SMALLINT NOT NULL,
    tile_v SMALLINT NOT NULL,
    granule_date TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    cloud_fraction DOUBLE PRECISION,
    valid_pixel_count BIGINT,
    rejected_pixel_count BIGINT,
    tile_set_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (ingest_id)
);

CREATE INDEX IF NOT EXISTS idx_processing_log_status ON processing_log(status);
CREATE INDEX IF NOT EXISTS idx_processing_log_tile_set ON processing_log(tile_set_id);
CREATE INDEX IF NOT EXISTS idx_processing_log_granule_date ON processing_log(granule_date DESC);
```

Allowed statuses: `processing`, `processed`, `rejected`, `failed`, `deadlettered`.

Current behavior upserts rows by unique `ingest_id` when a processing message is validated, increments `attempts`, sets status to `processing`, and records retry/dead-letter failures as `failed` or `deadlettered`. Science processing records sampled quality metadata and scalar summary columns. High-cloud granules are marked `rejected`. Accepted granules generate and publish a tile set, then terminal `processed` status is recorded with `tile_set_id` referencing the published immutable tile set.

## `tile_sets`

Added for Chunk 9 tile generation metadata:

```sql
CREATE TABLE IF NOT EXISTS tile_sets (
    id TEXT PRIMARY KEY,
    dataset_date DATE NOT NULL,
    classification_version TEXT NOT NULL,
    render_version TEXT NOT NULL,
    format TEXT NOT NULL,
    min_zoom SMALLINT NOT NULL,
    max_native_zoom SMALLINT NOT NULL,
    max_display_zoom SMALLINT NOT NULL,
    bounds JSONB NOT NULL,
    tile_count INTEGER NOT NULL,
    manifest_blob_path TEXT NOT NULL,
    latest BOOLEAN NOT NULL DEFAULT false,
    retention_deleted_at TIMESTAMPTZ,
    retention_delete_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tile_sets_latest_one
ON tile_sets(latest)
WHERE latest = true;

CREATE INDEX IF NOT EXISTS idx_tile_sets_dataset_date
ON tile_sets(dataset_date DESC);

CREATE INDEX IF NOT EXISTS idx_tile_sets_retention_cleanup
ON tile_sets(classification_version, created_at DESC)
WHERE retention_deleted_at IS NULL;
```

Promotion order:

1. Upload all tile blobs.
2. Upload immutable manifest.
3. Insert `tile_sets` row with `latest = false`.
4. In one transaction, set old latest false and new latest true.
5. Upload `manifests/latest.json` pointing to immutable manifest.
6. Record `processing_log.tile_set_id` and terminal `processed` status for the source processing message.

Retention cleanup marks stale whole tile sets with `retention_deleted_at` and `retention_delete_reason` only after selected tile blobs and the immutable manifest have been deleted or found missing. Latest tile sets are not retention-deleted.

## `retention_cleanup_events`

Added for Chunk 8 retention cleanup audit events:

```sql
CREATE TABLE IF NOT EXISTS retention_cleanup_events (
    id UUID PRIMARY KEY,
    cleanup_run_id UUID NOT NULL,
    mode TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_identifier TEXT NOT NULL,
    blob_container TEXT,
    blob_path TEXT,
    action TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_events_run
ON retention_cleanup_events(cleanup_run_id, created_at);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_events_target
ON retention_cleanup_events(target_kind, target_identifier);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_events_raw_completed
ON retention_cleanup_events(target_identifier)
WHERE target_kind = 'raw_blob'
  AND mode = 'execute'
  AND action IN ('deleted', 'missing');
```

Allowed modes: `dry_run`, `execute`. Allowed target kinds: `raw_blob`, `processed_tile`, `processed_manifest`, `tile_set`. Allowed actions: `selected`, `deleted`, `missing`, `skipped`.

## Future Public Site Tables

Add before implementing the deferred public observing site feature described in [../future/observing-sites-and-sky-quality.md](../future/observing-sites-and-sky-quality.md):

```sql
CREATE TABLE IF NOT EXISTS sites (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    elevation_m DOUBLE PRECISION,
    source TEXT NOT NULL,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (latitude >= -90 AND latitude <= 90),
    CHECK (longitude >= -180 AND longitude <= 180)
);

CREATE INDEX IF NOT EXISTS idx_sites_lat_lon ON sites(latitude, longitude);
CREATE INDEX IF NOT EXISTS idx_sites_source ON sites(source) WHERE deleted_at IS NULL;
```

If location search becomes slow or needs accurate radius calculations at scale, add PostGIS in a deliberate migration and update local/runtime dependencies.