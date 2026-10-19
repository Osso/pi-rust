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
//! no global fetch, so [`run_cli_main`] configures the process-wide dispatcher.
//!
//! `runCliActionOrReportError` lives in `main.ts`; callers pass its exit code
//! through [`CliMain`].

use std::future::Future;
use std::pin::Pin;

use crate::core::http_dispatcher::{DEFAULT_HTTP_IDLE_TIMEOUT_MS, configure_http_dispatcher};
use crate::internal_runner::{DetachedRunnerHost, run_internal_bwrap_fs_worker, run_internal_detached_runner};

/// Environment the TypeScript entry point sets before running `main`.
pub const CLI_PROCESS_ENV: [(&str, &str); 2] = [("PI_CODING_AGENT", "true"), ("AI_AGENT", "pi")];

/// `runCliActionOrReportError(() => main(process.argv.slice(2)))`.
pub type CliMain<'a> = Box<dyn FnOnce(&'a [String]) -> Pin<Box<dyn Future<Output = Result<i32, String>> + 'a>> + 'a>;

/// Entry point wiring: the internal bwrap worker, the detached runners, then the
/// agent CLI.
pub async fn run_cli_entry<'a>(
    args: &'a [String],
    detached_runner: &dyn DetachedRunnerHost,
    main: CliMain<'a>,
) -> Result<i32, String> {
    if let Some(exit_code) = run_internal_bwrap_fs_worker(args) {
        return Ok(exit_code);
    }
    if let Some(outcome) = run_internal_detached_runner(args, detached_runner).await? {
        return Ok(outcome.exit_code.unwrap_or(0));
    }
    run_cli_main(args, main).await
}

/// The non-internal-runner half of `cli.ts`: configure the dispatcher, then run
/// `main`.
pub async fn run_cli_main<'a>(args: &'a [String], main: CliMain<'a>) -> Result<i32, String> {
    configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS as f64)?;
    main(args).await
}
