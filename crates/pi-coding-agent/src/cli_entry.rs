//! CLI entry point (`cli.ts`).
//!
//! Three TypeScript steps have no direct Rust equivalent and are documented
//! deviations:
//!
//! - `process.title = APP_NAME` needs a platform-specific call and a new
//!   dependency.
//! - `process.emitWarning = () => {}` has no Rust analogue (Rust does not emit
//!   Node warnings).
//! - The process environment (`PI_CODING_AGENT`, `AI_AGENT`) is exported as
//!   [`CLI_PROCESS_ENV`] for the binary bootstrap to apply: edition 2024 makes
//!   `std::env::set_var` unsafe and this workspace denies unsafe code.
//!
//! `configureHttpDispatcher()` patches `globalThis.fetch` in TypeScript; Rust has
//! no global fetch, so [`run_cli_entry`] returns the configured [`Fetch`] for the
//! caller to hand to the model runtime.
//!
//! `runCliActionOrReportError` lives in `main.ts` (not yet ported); this entry
//! point propagates `main`'s error so the future port can wrap it there.

use std::sync::Arc;

use pi_ai::request::{BoxFuture, Fetch};

use crate::core::http_dispatcher::{DEFAULT_HTTP_IDLE_TIMEOUT_MS, configure_http_dispatcher};
use crate::internal_runner::{DetachedRunnerHost, run_internal_bwrap_fs_worker, run_internal_detached_runner};

/// Environment the TypeScript entry point sets before running `main`.
pub const CLI_PROCESS_ENV: [(&str, &str); 2] = [("PI_CODING_AGENT", "true"), ("AI_AGENT", "pi")];

/// `main(process.argv.slice(2))`.
pub type CliMain<'a> = Box<dyn FnOnce(&'a [String]) -> BoxFuture<'a, Result<(), String>> + 'a>;

/// Entry point wiring: internal detached runner first, then the agent CLI.
///
/// `Ok(None)` means an internal runner handled the process; `Ok(Some(fetch))`
/// is the configured fetch after `main` completed.
pub async fn run_cli_entry<'a>(
    args: &'a [String],
    detached_runner: &dyn DetachedRunnerHost,
    main: CliMain<'a>,
) -> Result<Option<Arc<dyn Fetch>>, String> {
    if let Some(exit_code) = run_internal_bwrap_fs_worker(args) {
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return Ok(None);
    }
    let handled = run_internal_detached_runner(args, detached_runner).await?;
    if handled.is_some() {
        return Ok(None);
    }
    let fetch = configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS as f64)?;
    main(args).await?;
    Ok(Some(fetch))
}
