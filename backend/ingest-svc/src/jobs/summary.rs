#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestSummary {
    pub discovered: usize,
    pub attempted: usize,
    pub downloaded: usize,
    pub validated: usize,
    pub enqueued: usize,
    pub rejected: usize,
    pub filtered_out_of_bounds: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl IngestSummary {
    pub(super) fn new(discovered: usize) -> Self {
        Self {
            discovered,
            attempted: 0,
            downloaded: 0,
            validated: 0,
            enqueued: 0,
            rejected: 0,
            filtered_out_of_bounds: 0,
            failed: 0,
            skipped: 0,
        }
    }

    pub(super) fn record_filtered_out_of_bounds(&mut self) {
        self.filtered_out_of_bounds += 1;
    }

    pub(super) fn record_attempt(&mut self, outcome: GranuleProcessingOutcome) {
        self.attempted += 1;

        match outcome {
            GranuleProcessingOutcome::Skipped => self.skipped += 1,
            GranuleProcessingOutcome::FailedBeforeDownloaded => self.failed += 1,
            GranuleProcessingOutcome::FailedAfterDownloaded => {
                self.downloaded += 1;
                self.failed += 1;
            }
            GranuleProcessingOutcome::RejectedAfterDownloaded => {
                self.downloaded += 1;
                self.rejected += 1;
            }
            GranuleProcessingOutcome::FailedAfterValidated => {
                self.downloaded += 1;
                self.validated += 1;
                self.failed += 1;
            }
            GranuleProcessingOutcome::Enqueued => {
                self.downloaded += 1;
                self.validated += 1;
                self.enqueued += 1;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GranuleProcessingOutcome {
    Skipped,
    FailedBeforeDownloaded,
    FailedAfterDownloaded,
    RejectedAfterDownloaded,
    FailedAfterValidated,
    Enqueued,
}
