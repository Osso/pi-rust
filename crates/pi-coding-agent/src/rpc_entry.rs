//! RPC entry point (`rpc-entry.ts`).
//!
//! TypeScript sets `process.title`, suppresses Node warnings, configures the
//! global fetch dispatcher, and calls `main` without awaiting it. The Rust port
//! keeps the environment write and the `--mode rpc` prefix, returns the
//! configured fetch, and propagates `main` errors; the platform-specific steps
//! are documented gaps shared with [`crate::cli_entry`].

use std::sync::Arc;

use pi_ai::request::{BoxFuture, Fetch};

use crate::cli_entry::{CLI_PROCESS_ENV, apply_process_env};
use crate::core::http_dispatcher::{DEFAULT_HTTP_IDLE_TIMEOUT_MS, configure_http_dispatcher};

/// Environment the TypeScript RPC entry point sets before running `main`.
pub const RPC_PROCESS_ENV: [(&str, &str); 1] = CLI_PROCESS_ENV;

/// Arguments prepended to the process arguments.
pub const RPC_MODE_ARGS: [&str; 2] = ["--mode", "rpc"];

/// `main(["--mode", "rpc", ...args])`.
pub type RpcMain<'a> = Box<dyn FnOnce(Vec<String>) -> BoxFuture<'a, Result<(), String>> + 'a>;

/// Configures the HTTP dispatcher and runs `main` with the RPC arguments.
pub async fn run_rpc_entry<'a>(args: &[String], main: RpcMain<'a>) -> Result<Arc<dyn Fetch>, String> {
    apply_process_env(&RPC_PROCESS_ENV);
    let fetch = configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS as f64)?;
    let mut main_args: Vec<String> = RPC_MODE_ARGS.iter().map(|arg| (*arg).to_owned()).collect();
    main_args.extend(args.iter().cloned());
    main(main_args).await?;
    Ok(fetch)
}
