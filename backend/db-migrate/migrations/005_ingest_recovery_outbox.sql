ALTER TABLE ingest_log
DROP CONSTRAINT IF EXISTS ck_ingest_log_status;

ALTER TABLE ingest_log
ADD CONSTRAINT ck_ingest_log_status CHECK (
    status IN (
        'downloading',
        'downloaded',
        'validated',
        'enqueued',
        'rejected',
        'failed',
        'recovery_pending',
        'replay_pending'
    )
);

CREATE TABLE IF NOT EXISTS ingest_recovery_outbox (
    id UUID PRIMARY KEY,
    ingest_id UUID NOT NULL REFERENCES ingest_log(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT ck_ingest_recovery_outbox_operation CHECK (
        operation IN ('enqueue_processing')
    ),
    CONSTRAINT ck_ingest_recovery_outbox_status CHECK (
        status IN ('pending', 'completed', 'failed')
    )
);

CREATE INDEX IF NOT EXISTS idx_ingest_recovery_outbox_status
ON ingest_recovery_outbox(status, created_at);

CREATE UNIQUE INDEX IF NOT EXISTS uq_ingest_recovery_outbox_pending_operation
ON ingest_recovery_outbox(ingest_id, operation)
WHERE status = 'pending';
