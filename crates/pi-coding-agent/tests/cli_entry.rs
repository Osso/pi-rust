//! `internal-runner.ts` and `cli.ts` entry wiring.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli_entry::{CLI_PROCESS_ENV, CliMain, run_cli_entry, run_cli_main};
use pi_coding_agent::internal_runner::{
    DETACHED_BASH_RUNNER_MODE, DETACHED_PYRUN_RUNNER_MODE, DetachedRunnerHost, run_internal_detached_runner,
};
use pi_js_compat::js_json::JsString;

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-internal-runner-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct ScriptedHost {
    bash: Result<(), JsString>,
    pyrun: Result<(), JsString>,
    calls: Mutex<Vec<JsString>>,
}

impl ScriptedHost {
    fn ok() -> Self {
        Self {
            bash: Ok(()),
            pyrun: Ok(()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<JsString> {
        self.calls.lock().unwrap().clone()
    }
}

impl DetachedRunnerHost for ScriptedHost {
    fn run_bash<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls.lock().unwrap().push(JsString::from("bash:") + manifest_path);
        let result = self.bash.clone();
        Box::pin(async move { result })
    }

    fn run_pyrun<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(JsString::from("pyrun:") + manifest_path);
        let result = self.pyrun.clone();
        Box::pin(async move { result })
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[tokio::test]
async fn other_argv_is_not_an_internal_runner() {
    let host = ScriptedHost::ok();
    assert_eq!(run_internal_detached_runner(&[], &host).await, Ok(None));
    assert_eq!(run_internal_detached_runner(&args(&["--help"]), &host).await, Ok(None));
    assert!(host.calls().is_empty());
}

#[tokio::test]
async fn missing_manifest_throws_the_exact_message() {
    let host = ScriptedHost::ok();
    for mode in [DETACHED_BASH_RUNNER_MODE, DETACHED_PYRUN_RUNNER_MODE] {
        assert_eq!(
            run_internal_detached_runner(&args(&[mode]), &host).await,
            Err(format!(
                "Internal detached runner {mode} requires a launch manifest path"
            ))
        );
    }
    assert!(host.calls().is_empty());
}

#[tokio::test]
async fn each_mode_dispatches_to_its_runner() {
    let host = ScriptedHost::ok();
    let outcome = run_internal_detached_runner(&args(&[DETACHED_BASH_RUNNER_MODE, "/tmp/manifest.json"]), &host)
        .await
        .unwrap()
        .expect("bash runner is handled");
    assert_eq!(outcome.exit_code, None);
    assert_eq!(outcome.runner_error, None);

    let outcome = run_internal_detached_runner(&args(&[DETACHED_PYRUN_RUNNER_MODE, "/tmp/manifest.json"]), &host)
        .await
        .unwrap()
        .expect("pyrun runner is handled");
    assert_eq!(outcome.exit_code, None);
    assert_eq!(
        host.calls(),
        vec!["bash:/tmp/manifest.json", "pyrun:/tmp/manifest.json"]
    );
}

#[tokio::test]
async fn runner_failure_writes_a_private_runner_error_file() {
    let dir = temp_dir("failure");
    let manifest = dir.join("launch.json");
    fs::write(&manifest, "{}").unwrap();
    let host = ScriptedHost {
        bash: Err("runner exploded".into()),
        pyrun: Ok(()),
        calls: Mutex::new(Vec::new()),
    };

    let outcome = run_internal_detached_runner(
        &args(&[DETACHED_BASH_RUNNER_MODE, &manifest.display().to_string()]),
        &host,
    )
    .await
    .unwrap()
    .expect("bash runner is handled");

    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(outcome.runner_error, Some(JsString::from("runner exploded")));
    let error_path = format!("{}.runner-error", manifest.display());
    assert_eq!(fs::read_to_string(&error_path).unwrap(), "runner exploded\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&error_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[tokio::test]
async fn success_does_not_create_an_error_file() {
    let dir = temp_dir("success");
    let manifest = dir.join("launch.json");
    fs::write(&manifest, "{}").unwrap();
    let host = ScriptedHost::ok();
    let outcome = run_internal_detached_runner(
        &args(&[DETACHED_PYRUN_RUNNER_MODE, &manifest.display().to_string()]),
        &host,
    )
    .await
    .unwrap()
    .expect("pyrun runner is handled");
    assert_eq!(outcome.exit_code, None);
    assert!(!PathBuf::from(format!("{}.runner-error", manifest.display())).exists());
}

#[tokio::test]
async fn cli_entry_stops_after_an_internal_runner_handles_the_process() {
    let host = ScriptedHost::ok();
    let main_called = Mutex::new(false);
    let argv = args(&[DETACHED_BASH_RUNNER_MODE, "/tmp/manifest.json"]);
    let main: CliMain<'_> = Box::new(|_args: &[String]| {
        *main_called.lock().unwrap() = true;
        Box::pin(async { Ok(0) })
    });

    let result = run_cli_entry(&argv, &host, main).await.unwrap();

    assert_eq!(result, 0);
    assert!(!*main_called.lock().unwrap());
}

#[tokio::test]
async fn cli_entry_runs_main_and_returns_its_exit_code() {
    let host = ScriptedHost::ok();
    let seen = Mutex::new(Vec::new());
    let argv = args(&["--help"]);
    let main: CliMain<'_> = Box::new(|args: &[String]| {
        seen.lock().unwrap().extend(args.iter().cloned());
        Box::pin(async { Ok(7) })
    });

    let result = run_cli_entry(&argv, &host, main).await.unwrap();

    assert_eq!(result, 7);
    assert_eq!(*seen.lock().unwrap(), vec!["--help".to_owned()]);
    assert!(host.calls().is_empty());
}

#[tokio::test]
async fn cli_entry_propagates_main_errors() {
    let host = ScriptedHost::ok();
    let argv = args(&[]);
    let main: CliMain<'_> = Box::new(|_args: &[String]| Box::pin(async { Err("boom".to_owned()) }));
    let error = run_cli_entry(&argv, &host, main)
        .await
        .expect_err("main error propagates");
    assert_eq!(error, "boom");
}

#[test]
fn cli_process_env_matches_the_typescript_entry_point() {
    assert_eq!(CLI_PROCESS_ENV, [("PI_CODING_AGENT", "true")]);
}

#[tokio::test]
async fn run_cli_main_exports_the_typescript_process_environment_to_children() {
    pi_js_compat::process_env::clear();
    pi_js_compat::process_env::set("AI_AGENT", "inherited-agent");
    let argv = args(&[]);
    let main: CliMain<'_> = Box::new(|_args: &[String]| Box::pin(async { Ok(0) }));
    assert_eq!(run_cli_main(&argv, main).await.expect("cli main"), 0);

    let output = pi_coding_agent::utils::child_process::spawn_sync(
        &"sh".into(),
        &["-c".into(), "printf '%s|%s' \"$PI_CODING_AGENT\" \"$AI_AGENT\"".into()],
        Default::default(),
    )
    .expect("child process");
    assert!(output.error.is_none());
    assert_eq!(
        String::from_utf8(output.stdout.unwrap()).unwrap(),
        "true|inherited-agent"
    );
    assert_eq!(
        pi_js_compat::process_env::get("PI_CODING_AGENT").as_deref(),
        Some("true")
    );
    assert_eq!(
        pi_js_compat::process_env::get("AI_AGENT").as_deref(),
        Some("inherited-agent")
    );
    pi_js_compat::process_env::clear();
}
