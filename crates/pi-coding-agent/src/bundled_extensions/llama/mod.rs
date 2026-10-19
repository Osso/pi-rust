//! Port of `packages/coding-agent/src/extensions/llama/**`.
//!
//! The llama.cpp extension manages a local router over HTTP and registers the
//! `llama.cpp` native provider.

pub mod client;
pub mod huggingface;
pub mod index;
pub mod provider;
pub mod ui;
