ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS retention_deleted_at TIMESTAMPTZ;

ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS retention_delete_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_tile_sets_retention_cleanup
ON tile_sets(classification_version, created_at DESC)
WHERE retention_deleted_at IS NULL;

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
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_retention_cleanup_events_mode
        CHECK (mode IN ('dry_run', 'execute')),
    CONSTRAINT ck_retention_cleanup_events_target_kind
        CHECK (target_kind IN ('raw_blob', 'processed_tile', 'processed_manifest', 'tile_set')),
    CONSTRAINT ck_retention_cleanup_events_action
        CHECK (action IN ('selected', 'deleted', 'missing', 'skipped'))
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
