//! Library facade for pichost-worker.
//!
//! Exposes internal modules so integration tests under `tests/` (and future
//! consumers) can exercise them through the crate's public API. The binary
//! entry point (`main.rs`) declares its own `mod` declarations and is built
//! independently.
pub mod queue;
