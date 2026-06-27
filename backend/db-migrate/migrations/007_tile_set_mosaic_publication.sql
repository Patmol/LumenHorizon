ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS product TEXT;

ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS cadence TEXT;

ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS tile_set_kind TEXT NOT NULL DEFAULT 'granule';

ALTER TABLE tile_sets
ADD COLUMN IF NOT EXISTS product_latest BOOLEAN NOT NULL DEFAULT false;

UPDATE tile_sets
SET product = source.product
FROM (
    SELECT DISTINCT ON (tile_set_id)
        tile_set_id,
        product
    FROM processing_log
    WHERE tile_set_id IS NOT NULL
    ORDER BY tile_set_id, updated_at DESC, id DESC
) AS source
WHERE tile_sets.id = source.tile_set_id
  AND tile_sets.product IS NULL;

UPDATE tile_sets
SET cadence = CASE
        WHEN product IN ('VNP46A2', 'VJ146A2') THEN 'daily'
        WHEN product = 'VNP46A3' THEN 'monthly'
        ELSE cadence
    END
WHERE cadence IS NULL
  AND product IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'ck_tile_sets_tile_set_kind'
          AND conrelid = 'tile_sets'::regclass
    ) THEN
        ALTER TABLE tile_sets
        ADD CONSTRAINT ck_tile_sets_tile_set_kind
        CHECK (tile_set_kind IN ('granule', 'mosaic'));
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'ck_tile_sets_cadence'
          AND conrelid = 'tile_sets'::regclass
    ) THEN
        ALTER TABLE tile_sets
        ADD CONSTRAINT ck_tile_sets_cadence
        CHECK (cadence IS NULL OR cadence IN ('daily', 'monthly'));
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_tile_sets_product_date
ON tile_sets(product, dataset_date DESC, created_at DESC, id DESC)
WHERE retention_deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_tile_sets_kind
ON tile_sets(tile_set_kind, created_at DESC)
WHERE retention_deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_tile_sets_product_latest_one
ON tile_sets(product, classification_version, render_version)
WHERE product_latest = true
  AND product IS NOT NULL
  AND retention_deleted_at IS NULL;
