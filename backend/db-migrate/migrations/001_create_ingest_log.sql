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
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_ingest_log_status CHECK (
        status IN (
            'downloading',
            'downloaded',
            'validated',
            'enqueued',
            'rejected',
            'failed'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_ingest_log_status ON ingest_log(status);
CREATE INDEX IF NOT EXISTS idx_ingest_log_granule_date ON ingest_log(granule_date DESC);
CREATE INDEX IF NOT EXISTS idx_ingest_log_product_date ON ingest_log(product, granule_date DESC);