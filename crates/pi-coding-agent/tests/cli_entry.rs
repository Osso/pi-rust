//! `internal-runner.ts` and `cli.ts` entry wiring.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli_entry::{CLI_PROCESS_ENV, CliMain, run_cli_entry};
use pi_coding_agent::internal_runner::{
    DETACHED_BASH_RUNNER_MODE, DETACHED_PYRUN_RUNNER_MODE, DetachedRunnerHost, run_internal_detached_runner,
};

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
    bash: Result<(), String>,
    pyrun: Result<(), String>,
    calls: Mutex<Vec<String>>,
}

impl ScriptedHost {
    fn ok() -> Self {
        Self {
            bash: Ok(()),
            pyrun: Ok(()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl DetachedRunnerHost for ScriptedHost {
    fn run_bash<'a>(&'a self, manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        self.calls.lock().unwrap().push(format!("bash:{manifest_path}"));
        let result = self.bash.clone();
        Box::pin(async move { result })
    }

    fn run_pyrun<'a>(&'a self, manifest_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        self.calls.lock().unwrap().push(format!("pyrun:{manifest_path}"));
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
        bash: Err("runner exploded".to_owned()),
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
    assert_eq!(outcome.runner_error.as_deref(), Some("runner exploded"));
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
        Box::pin(async { Ok(()) })
    });

    let result = run_cli_entry(&argv, &host, main).await.unwrap();

    assert!(result.is_none());
    assert!(!*main_called.lock().unwrap());
}

#[tokio::test]
async fn cli_entry_runs_main_and_returns_the_configured_fetch() {
    let host = ScriptedHost::ok();
    let seen = Mutex::new(Vec::new());
    let argv = args(&["--help"]);
    let main: CliMain<'_> = Box::new(|args: &[String]| {
        seen.lock().unwrap().extend(args.iter().cloned());
        Box::pin(async { Ok(()) })
    });

    let result = run_cli_entry(&argv, &host, main).await.unwrap();
    let fetch = result.expect("main path returns the configured fetch");
    assert!(
        fetch
            .fetch(
                pi_ai::request::HttpRequest {
                    method: "GET".to_owned(),
                    url: "http://127.0.0.1:1/never".to_owned(),
                    headers: Vec::new(),
                    body: None,
                },
                Some(tokio_util::sync::CancellationToken::new()),
            )
            .await
            .is_err()
    );
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
        .err()
        .expect("main error propagates");
    assert_eq!(error, "boom");
}

#[test]
fn cli_process_env_matches_the_typescript_entry_point() {
    assert_eq!(CLI_PROCESS_ENV, [("PI_CODING_AGENT", "true"), ("AI_AGENT", "pi")]);
}
