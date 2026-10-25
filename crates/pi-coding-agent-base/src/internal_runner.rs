//! Shared internal runner invocation modes.

/// `DETACHED_BASH_RUNNER_MODE`.
pub const DETACHED_BASH_RUNNER_MODE: &str = "--internal-detached-bash-runner";
/// `DETACHED_PYRUN_RUNNER_MODE`.
pub const DETACHED_PYRUN_RUNNER_MODE: &str = "--internal-detached-pyrun-runner";
/// Rust replacement for bwrap's Node `fs-worker.cjs` (`--internal-bwrap-fs-worker`).
pub const BWRAP_FS_WORKER_MODE: &str = "--internal-bwrap-fs-worker";
