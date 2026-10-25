//! `pi` executable (`cli.ts` plus the `main.ts` orchestration).
//!
//! Platform-specific Node steps (`process.title`, warning suppression, process
//! environment mutation) are documented deviations; see
//! `docs/porting/traceability/main.md`.

use std::process::ExitCode;

use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::pyrun::detached_runner::run_detached_pyrun_runner;
use pi_coding_agent::cli_entry::{CliMain, run_cli_entry};
use pi_coding_agent::core::detached_bash_runner::run_detached_bash_runner;
use pi_coding_agent::internal_runner::DetachedRunnerHost;
use pi_coding_agent::main_runner::{MainOptions, console_error, run_main_or_report_error};
use pi_coding_agent::utils::chalk::Chalk;
use pi_js_compat::js_json::JsString;

/// Internal runner host backed by the ported detached bash runner.
struct CliDetachedRunnerHost;

impl DetachedRunnerHost for CliDetachedRunnerHost {
    fn run_bash<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move {
            run_detached_bash_runner(manifest_path, None)
                .await
                .map(|_| ())
                .map_err(|error| error.message().clone())
        })
    }

    fn run_pyrun<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move { run_detached_pyrun_runner(manifest_path).await.map(|_| ()) })
    }
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::bundled_extensions::agents_core::runtime::default_browser_agent_config_root()
        .unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_native_uncaught(error));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let main: CliMain<'_> =
        Box::new(|args| Box::pin(async move { Ok(run_main_or_report_error(args, MainOptions::default()).await) }));
    match run_cli_entry(&args, &CliDetachedRunnerHost, main).await {
        Ok(code) => exit_code(code),
        Err(message) => {
            console_error(Chalk::stderr().red(format!("Error: {message}")));
            exit_code(1)
        }
    }
}
