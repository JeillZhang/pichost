//! Library facade for pichost-worker.
//!
//! Exposes the image-processing pipeline as a public API so integration
//! tests under `tests/` and in-process consumers (e.g. the lite-mode
//! embedded worker) can call `process_task` directly. The binary entry
//! point (`main.rs`) declares its own `mod` declarations and is built
//! independently — both targets compile the same module sources.
pub mod db;
pub mod fonts;
pub mod pipeline;
pub mod processor;
pub mod queue;
pub mod watermark;

pub use pipeline::process_task;
