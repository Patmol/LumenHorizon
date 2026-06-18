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
    UNIQUE (ingest_id),
    CONSTRAINT ck_processing_log_status
        CHECK (status IN ('processing', 'processed', 'rejected', 'failed', 'deadlettered')),
    CONSTRAINT ck_processing_log_attempts_nonnegative
        CHECK (attempts >= 0),
    CONSTRAINT ck_processing_log_cloud_fraction_range
        CHECK (cloud_fraction IS NULL OR (cloud_fraction >= 0 AND cloud_fraction <= 1)),
    CONSTRAINT ck_processing_log_valid_pixel_count_nonnegative
        CHECK (valid_pixel_count IS NULL OR valid_pixel_count >= 0),
    CONSTRAINT ck_processing_log_rejected_pixel_count_nonnegative
        CHECK (rejected_pixel_count IS NULL OR rejected_pixel_count >= 0)
);

CREATE INDEX IF NOT EXISTS idx_processing_log_status ON processing_log(status);
CREATE INDEX IF NOT EXISTS idx_processing_log_tile_set ON processing_log(tile_set_id);
CREATE INDEX IF NOT EXISTS idx_processing_log_granule_date ON processing_log(granule_date DESC);