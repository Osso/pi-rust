//! `pi` executable (`cli.ts` plus the `main.ts` orchestration).
//!
//! Platform-specific Node steps (`process.title`, warning suppression, process
//! environment mutation) are documented deviations; see
//! `docs/porting/traceability/main.md`.

use std::process::ExitCode;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli_entry::{CliMain, run_cli_main};
use pi_coding_agent::core::detached_bash_runner::run_detached_bash_runner;
use pi_coding_agent::internal_runner::{DetachedRunnerHost, run_internal_detached_runner};
use pi_coding_agent::main_runner::{MainOptions, console_error, run_main_or_report_error};
use pi_coding_agent::utils::chalk::Chalk;

/// Internal runner host backed by the ported detached bash runner.
struct CliDetachedRunnerHost;

impl DetachedRunnerHost for CliDetachedRunnerHost {
    fn run_bash<'a>(&'a self, manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            run_detached_bash_runner(manifest_path, None)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }

    fn run_pyrun<'a>(&'a self, _manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async {
            Err("pyrun detached runner is not ported (extensions/pyrun/src/detached-runner.ts)".to_owned())
        })
    }
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_internal_detached_runner(&args, &CliDetachedRunnerHost).await {
        Ok(Some(outcome)) => return exit_code(outcome.exit_code.unwrap_or(0)),
        Ok(None) => {}
        Err(message) => {
            console_error(&Chalk::stderr().red(&format!("Error: {message}")));
            return exit_code(1);
        }
    }

    let main: CliMain<'_> =
        Box::new(|args| Box::pin(async move { Ok(run_main_or_report_error(args, MainOptions::default()).await) }));
    match run_cli_main(&args, main).await {
        Ok(code) => exit_code(code),
        Err(message) => {
            console_error(&Chalk::stderr().red(&format!("Error: {message}")));
            exit_code(1)
        }
    }
}
