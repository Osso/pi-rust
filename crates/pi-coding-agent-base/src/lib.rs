//! Shared configuration and utility leaves of the coding-agent runtime.

pub mod config;
pub mod console;
pub mod internal_runner;
pub mod utils;

// Preserve utility paths while moving the console implementation below them.
pub use console as main_runner;
