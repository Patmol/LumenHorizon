DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_ingest_log_product_tile_date'
          AND conrelid = 'ingest_log'::regclass
    ) THEN
        ALTER TABLE ingest_log
        ADD CONSTRAINT uq_ingest_log_product_tile_date
        UNIQUE (product, tile_h, tile_v, granule_date);
    END IF;
END
$$;