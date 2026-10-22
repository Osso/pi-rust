//! `src/bun/` entry shims: Bedrock registration and the standalone-binary entry.

use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::bun::cli::run_bun_cli_entry;
use pi_coding_agent::bun::register_bedrock::register_bedrock;
use pi_coding_agent::cli_entry::CliMain;
use pi_coding_agent::internal_runner::{DETACHED_BASH_RUNNER_MODE, DetachedRunnerHost};
use pi_js_compat::js_json::JsString;

/// `registerBedrock` installs the real module; the override slot itself is
/// covered by `pi-ai`'s `bedrock_provider` tests. This asserts the shim is
/// callable and idempotent (the binary runs it once per process).
#[test]
fn register_bedrock_is_idempotent() {
    register_bedrock();
    register_bedrock();
}

struct RecordingRunner {
    calls: Mutex<Vec<JsString>>,
}

impl DetachedRunnerHost for RecordingRunner {
    fn run_bash<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls.lock().unwrap().push(JsString::from("bash:") + manifest_path);
        Box::pin(async { Ok(()) })
    }

    fn run_pyrun<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(JsString::from("pyrun:") + manifest_path);
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn bun_cli_entry_registers_and_hands_off_to_the_cli() {
    let runner = RecordingRunner {
        calls: Mutex::new(Vec::new()),
    };
    let args = vec![DETACHED_BASH_RUNNER_MODE.to_owned(), "/tmp/manifest.json".to_owned()];
    let main_called = Mutex::new(false);
    let main: CliMain<'_> = Box::new(|_args: &[String]| {
        *main_called.lock().unwrap() = true;
        Box::pin(async { Ok(0) })
    });

    let result = run_bun_cli_entry(&args, &runner, main).await.unwrap();
    assert_eq!(result, 0);
    assert!(!*main_called.lock().unwrap());
    assert_eq!(
        runner.calls.lock().unwrap().clone(),
        vec!["bash:/tmp/manifest.json".to_owned()]
    );
}

#[tokio::test]
async fn bun_cli_entry_runs_main_when_no_internal_runner_matches() {
    let runner = RecordingRunner {
        calls: Mutex::new(Vec::new()),
    };
    let args = vec!["--version".to_owned()];
    let seen = Mutex::new(Vec::new());
    let main: CliMain<'_> = Box::new(|args: &[String]| {
        seen.lock().unwrap().extend(args.iter().cloned());
        Box::pin(async { Ok(0) })
    });

    let result = run_bun_cli_entry(&args, &runner, main).await.unwrap();
    assert_eq!(result, 0, "main path returns its exit code");
    assert_eq!(*seen.lock().unwrap(), vec!["--version".to_owned()]);
    assert!(runner.calls.lock().unwrap().is_empty());
}
