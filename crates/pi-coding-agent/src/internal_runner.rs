//! Internal detached runner dispatch (`internal-runner.ts`).
//!
//! The two runner bodies (`core/detached-bash-runner.ts` and the pyrun
//! `detached-runner.ts`) are owned by other slices; this module ports the
//! dispatch, manifest validation, and `.runner-error` reporting.

use std::fs::OpenOptions;
use std::io::Write;

use pi_ai::request::BoxFuture;

/// `DETACHED_BASH_RUNNER_MODE`.
pub const DETACHED_BASH_RUNNER_MODE: &str = "--internal-detached-bash-runner";
/// `DETACHED_PYRUN_RUNNER_MODE`.
pub const DETACHED_PYRUN_RUNNER_MODE: &str = "--internal-detached-pyrun-runner";

/// The runner bodies this module dispatches to.
pub trait DetachedRunnerHost: Send + Sync {
    fn run_bash<'a>(&'a self, manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>>;
    fn run_pyrun<'a>(&'a self, manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// Outcome of a handled runner invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalRunnerOutcome {
    /// `process.exitCode = 1` after a failed run.
    pub exit_code: Option<i32>,
    /// The message written to `<manifest>.runner-error`, when the run failed.
    pub runner_error: Option<String>,
}

/// `runInternalDetachedRunner`.
///
/// `Ok(None)` means the argv does not select an internal runner. `Err` is the
/// thrown missing-manifest error; a runner failure is reported through the
/// outcome and the error file.
pub async fn run_internal_detached_runner(
    args: &[String],
    host: &dyn DetachedRunnerHost,
) -> Result<Option<InternalRunnerOutcome>, String> {
    let Some(mode) = args.first() else {
        return Ok(None);
    };
    if mode != DETACHED_BASH_RUNNER_MODE && mode != DETACHED_PYRUN_RUNNER_MODE {
        return Ok(None);
    }
    let Some(manifest_path) = args.get(1) else {
        return Err(format!(
            "Internal detached runner {mode} requires a launch manifest path"
        ));
    };

    let result = if mode == DETACHED_BASH_RUNNER_MODE {
        host.run_bash(manifest_path).await
    } else {
        host.run_pyrun(manifest_path).await
    };
    match result {
        Ok(()) => Ok(Some(InternalRunnerOutcome {
            exit_code: None,
            runner_error: None,
        })),
        Err(message) => {
            let error_path = format!("{manifest_path}.runner-error");
            write_runner_error(&error_path, &message)?;
            Ok(Some(InternalRunnerOutcome {
                exit_code: Some(1),
                runner_error: Some(message),
            }))
        }
    }
}

/// `writeFileSync(path, message + "\n", { encoding: "utf8", mode: 0o600 })`.
fn write_runner_error(path: &str, message: &str) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    writeln!(file, "{message}").map_err(|error| error.to_string())
}
