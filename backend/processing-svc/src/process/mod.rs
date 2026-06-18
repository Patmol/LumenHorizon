//! Processing command dispatch and queue-message handling.
//!
//! The process module owns the service flow from a processing request through
//! quality checks, tile generation, publication, and final status updates.

mod dispatch;
mod failure;
mod message;
mod metadata;
mod paths;
mod queue_worker;

pub(crate) use dispatch::dispatch;

#[cfg(test)]
mod tests;
