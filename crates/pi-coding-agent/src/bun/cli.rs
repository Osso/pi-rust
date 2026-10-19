//! `bun/cli.ts`: the standalone-binary entry shim.
//!
//! Ported steps: `registerBunOAuthFlows()`, `restoreSandboxEnv()`, the Bedrock
//! provider registration, and the `cli.ts` hand-off.
//!
//! Documented gaps:
//! - `restoreSandboxEnv()` repairs a Bun-only bug (empty `process.env` inside
//!   sandboxes, oven-sh/bun#27802). Rust binaries do not run under Bun, and
//!   edition 2024 makes `std::env::set_var` unsafe, so there is nothing to
//!   restore.
//! - `process.title` and `process.emitWarning` are documented in
//!   [`crate::cli_entry`].

use crate::cli_entry::{CliMain, run_cli_entry};
use crate::internal_runner::DetachedRunnerHost;

/// `bun/cli.ts` wiring: bundled OAuth flows, Bedrock module, then `cli.ts`.
pub async fn run_bun_cli_entry<'a>(
    args: &'a [String],
    detached_runner: &dyn DetachedRunnerHost,
    main: CliMain<'a>,
) -> Result<i32, String> {
    pi_ai::bun_oauth::register_bun_oauth_flows();
    super::register_bedrock::register_bedrock();
    run_cli_entry(args, detached_runner, main).await
}
