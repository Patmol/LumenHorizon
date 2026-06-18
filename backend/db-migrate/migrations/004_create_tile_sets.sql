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
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tile_sets_latest_one
ON tile_sets(latest)
WHERE latest = true;

CREATE INDEX IF NOT EXISTS idx_tile_sets_dataset_date
ON tile_sets(dataset_date DESC);