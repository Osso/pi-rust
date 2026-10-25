use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::detached_job_bootstrap::{
    DetachedPayloadError, GatedDetachedPayloadInput, spawn_gated_detached_payload,
};

#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn existing_output_error_matches_frozen_node_open() {
    let fixture = Fixture::new();
    fs::write(fixture.output(), "keep").unwrap();
    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let expected = oracle.request(&pi_js_compat::js_json!({
        "id": 28201, "module":"core/detached-job-bootstrap.ts", "operation":"detachedBootstrapError",
        "input": {
            "args": [], "command": fixture.command, "cwd": fixture.cwd,
            "identityPath": fixture.identity, "stderrPath": fixture.output, "stdoutPath": fixture.output,
        },
    }));
    assert_eq!(expected["ok"], pi_js_compat::js_json::JsValue::Bool(true));
    let error = spawn_gated_detached_payload(&payload_input(&fixture, &[])).unwrap_err();
    assert_eq!(error.message(), expected["output"].as_string().unwrap().clone());
    assert_eq!(fs::read_to_string(fixture.output()).unwrap(), "keep");
}

struct Fixture {
    directory: tempfile::TempDir,
    output: JsString,
    identity: JsString,
    cwd: JsString,
    command: JsString,
    marker: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let output = JsString::from(directory.path().join("job.output").to_str().expect("utf-8 test path"));
        let identity = JsString::from(directory.path().join("payload.json").to_str().expect("utf-8 test path"));
        let cwd = JsString::from(directory.path().to_str().expect("utf-8 test path"));
        let marker = directory.path().join("payload-ran");
        Self {
            directory,
            output,
            identity,
            cwd,
            command: "/bin/sh".into(),
            marker,
        }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn output(&self) -> PathBuf {
        encode_node_utf8(&self.output).into()
    }

    fn identity(&self) -> PathBuf {
        encode_node_utf8(&self.identity).into()
    }

    fn marker(&self) -> &Path {
        &self.marker
    }
}

fn payload_input<'a>(fixture: &'a Fixture, args: &'a [JsString]) -> GatedDetachedPayloadInput<'a> {
    GatedDetachedPayloadInput {
        args,
        command: &fixture.command,
        cwd: &fixture.cwd,
        env: None,
        identity_path: &fixture.identity,
        stderr_path: &fixture.output,
        stdout_path: &fixture.output,
    }
}

fn payload_args(fixture: &Fixture) -> Vec<JsString> {
    vec![
        "-c".into(),
        pi_js_compat::js_format!(
            "printf ran > '{}'; printf output",
            fixture.marker().to_str().expect("utf-8 test path")
        ),
    ]
}

#[tokio::test]
async fn persists_identity_before_releasing_payload_with_direct_artifact_descriptors() {
    let fixture = Fixture::new();
    let args = payload_args(&fixture);
    let input = payload_input(&fixture, &args);
    let mut payload = spawn_gated_detached_payload(&input).expect("payload must spawn");
    let pid = payload.pid();

    assert!(!fixture.marker().exists(), "the gate must hold the payload");
    assert_eq!(fs::read_link(format!("/proc/{pid}/fd/1")).unwrap(), fixture.output());
    assert_eq!(fs::read_link(format!("/proc/{pid}/fd/2")).unwrap(), fixture.output());
    assert!(
        fs::read_to_string(format!("/proc/{pid}/status"))
            .unwrap()
            .contains(&format!("PPid:\t{}", std::process::id())),
        "the payload must stay a child of this process"
    );
    assert_eq!(
        wait_for_session_leader(pid),
        pid.to_string(),
        "setsid must make the payload a session leader"
    );

    payload.persist_identity().expect("identity must persist");
    let identity: serde_json::Value = serde_json::from_str(&fs::read_to_string(fixture.identity()).unwrap()).unwrap();
    assert_eq!(identity["pid"], pid);
    assert_eq!(identity["pgid"], pid);
    assert!(identity["startTimeTicks"].as_u64().unwrap() > 0);
    assert!(
        !fixture.identity().with_extension("json.tmp").exists(),
        "identity must be renamed into place"
    );
    assert_eq!(
        fs::metadata(fixture.identity()).unwrap().permissions().mode() & 0o777,
        0o600
    );

    payload.release().expect("release must succeed");
    let exit = payload.wait_for_exit().await.expect("wait must succeed");
    assert_eq!(exit.exit_code, Some(0));
    assert_eq!(exit.signal, None);
    assert_eq!(fs::read_to_string(fixture.marker()).unwrap(), "ran");
    assert!(fs::read_to_string(fixture.output()).unwrap().contains("output"));
}

#[tokio::test]
async fn refuses_to_release_before_persisting_the_identity() {
    let fixture = Fixture::new();
    let args = payload_args(&fixture);
    let mut payload = spawn_gated_detached_payload(&payload_input(&fixture, &args)).unwrap();

    let error = payload.release().expect_err("release must be refused");
    assert_eq!(
        error.to_string(),
        "Detached payload identity must be persisted before release"
    );
    assert!(matches!(error, DetachedPayloadError::IdentityNotPersisted));
    assert!(!fixture.marker().exists());
    payload.persist_identity().unwrap();
    payload.release().unwrap();
    payload.wait_for_exit().await.unwrap();
}

#[tokio::test]
async fn refuses_a_second_release() {
    let fixture = Fixture::new();
    let args = payload_args(&fixture);
    let mut payload = spawn_gated_detached_payload(&payload_input(&fixture, &args)).unwrap();
    payload.persist_identity().unwrap();
    payload.release().unwrap();
    assert_eq!(
        payload.release().expect_err("second release must fail").to_string(),
        "Detached payload gate was already released"
    );
    payload.wait_for_exit().await.unwrap();
}

#[tokio::test]
async fn rejects_split_output_files() {
    let fixture = Fixture::new();
    let args = payload_args(&fixture);
    let stderr = JsString::from(fixture.root().join("stderr.output").to_str().expect("utf-8 test path"));
    let mut input = payload_input(&fixture, &args);
    input.stderr_path = &stderr;
    assert_eq!(
        spawn_gated_detached_payload(&input)
            .expect_err("split output must be rejected")
            .to_string(),
        "Split detached payload output is not implemented"
    );
}

#[tokio::test]
async fn rejects_an_existing_output_file() {
    let fixture = Fixture::new();
    fs::write(fixture.output(), "existing").unwrap();
    let args = payload_args(&fixture);
    assert!(spawn_gated_detached_payload(&payload_input(&fixture, &args)).is_err());
    assert_eq!(fs::read_to_string(fixture.output()).unwrap(), "existing");
}

#[tokio::test]
async fn reports_an_unusable_cwd_as_a_spawn_failure() {
    let fixture = Fixture::new();
    let args: Vec<JsString> = Vec::new();
    let missing_cwd = JsString::from(fixture.root().join("missing-cwd").to_str().expect("utf-8 test path"));
    let mut input = payload_input(&fixture, &args);
    input.cwd = &missing_cwd;
    assert_eq!(
        spawn_gated_detached_payload(&input)
            .expect_err("missing cwd must fail")
            .to_string(),
        "Could not start detached payload: /bin/sh"
    );
}

#[tokio::test]
async fn a_missing_command_fails_inside_the_shell_after_release() {
    let fixture = Fixture::new();
    let args: Vec<JsString> = Vec::new();
    let command = JsString::from("/nonexistent/pi-detached");
    let mut input = payload_input(&fixture, &args);
    input.command = &command;
    let mut payload = spawn_gated_detached_payload(&input).expect("the shell always spawns");
    payload.persist_identity().unwrap();
    payload.release().unwrap();
    let exit = payload.wait_for_exit().await.unwrap();
    assert_eq!(exit.exit_code, Some(127));
    assert_eq!(exit.signal, None);
}

fn install_child_executable(directory: &Path) {
    let child = directory.join("pi-review-child");
    fs::copy("/usr/bin/true", &child).unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
}

async fn run_to_exit(input: GatedDetachedPayloadInput<'_>, label: &str, root: &Path) -> Option<i32> {
    let output = JsString::from(root.join(format!("{label}.output")).to_str().expect("utf-8 test path"));
    let identity = JsString::from(
        root.join(format!("{label}.identity"))
            .to_str()
            .expect("utf-8 test path"),
    );
    let input = GatedDetachedPayloadInput {
        stdout_path: &output,
        stderr_path: &output,
        identity_path: &identity,
        ..input
    };
    let mut payload = spawn_gated_detached_payload(&input).expect("payload must spawn");
    payload.persist_identity().unwrap();
    payload.release().unwrap();
    payload.wait_for_exit().await.unwrap().exit_code
}

#[tokio::test]
async fn resolves_a_relative_command_against_the_manifest_cwd() {
    let fixture = Fixture::new();
    let child_cwd = fixture.root().join("child");
    fs::create_dir(&child_cwd).unwrap();
    install_child_executable(&child_cwd);
    let args: Vec<JsString> = Vec::new();
    let command = JsString::from("./pi-review-child");
    let cwd = JsString::from(child_cwd.to_str().expect("utf-8 test path"));
    let mut input = payload_input(&fixture, &args);
    input.command = &command;
    input.cwd = &cwd;
    assert_eq!(run_to_exit(input, "child-cwd", fixture.root()).await, Some(0));
}

#[tokio::test]
async fn resolves_a_bare_command_against_the_manifest_path() {
    let fixture = Fixture::new();
    let child_cwd = fixture.root().join("child");
    fs::create_dir(&child_cwd).unwrap();
    install_child_executable(&child_cwd);
    let args: Vec<JsString> = Vec::new();
    let env = vec![(
        JsString::from("PATH"),
        JsString::from(child_cwd.to_str().expect("utf-8 test path")),
    )];
    let command = JsString::from("pi-review-child");
    let cwd = JsString::from(child_cwd.to_str().expect("utf-8 test path"));
    let mut input = payload_input(&fixture, &args);
    input.command = &command;
    input.cwd = &cwd;
    input.env = Some(&env);
    assert_eq!(run_to_exit(input, "child-path", fixture.root()).await, Some(0));
}

#[tokio::test]
async fn replaces_the_child_environment_with_the_manifest_environment() {
    const PARENT_ONLY: &str = "PI_AUDIT_PARENT_ONLY";
    pi_js_compat::process_env::set(PARENT_ONLY, "parent-marker");
    let fixture = Fixture::new();
    let script = format!("printf '%s|%s' \"${{{PARENT_ONLY}-unset}}\" \"${{PI_AUDIT_MANIFEST_ONLY-unset}}\"");
    let args: Vec<JsString> = vec!["-c".into(), script.into()];
    let env = vec![("PI_AUDIT_MANIFEST_ONLY".into(), "manifest-marker".into())];
    let mut input = payload_input(&fixture, &args);
    input.env = Some(&env);
    assert_eq!(run_to_exit(input, "env", fixture.root()).await, Some(0));
    assert_eq!(
        fs::read_to_string(fixture.root().join("env.output")).unwrap(),
        "unset|manifest-marker"
    );
}

#[tokio::test]
async fn inherits_the_process_environment_when_the_manifest_has_none() {
    const PARENT_ONLY: &str = "PI_AUDIT_INHERITED";
    pi_js_compat::process_env::set(PARENT_ONLY, "inherited-marker");
    let fixture = Fixture::new();
    let args: Vec<JsString> = vec!["-c".into(), format!("printf '%s' \"${{{PARENT_ONLY}-unset}}\"").into()];
    let input = payload_input(&fixture, &args);
    assert_eq!(run_to_exit(input, "inherit", fixture.root()).await, Some(0));
    assert_eq!(
        fs::read_to_string(fixture.root().join("inherit.output")).unwrap(),
        "inherited-marker"
    );
}

#[tokio::test]
async fn gives_the_payload_a_closed_stdin_after_release() {
    let fixture = Fixture::new();
    let args = vec!["-c".into(), "read line || printf eof".into()];
    let mut payload = spawn_gated_detached_payload(&payload_input(&fixture, &args)).unwrap();
    payload.persist_identity().unwrap();
    payload.release().unwrap();
    let exit = payload.wait_for_exit().await.unwrap();
    assert_eq!(exit.exit_code, Some(0));
    assert_eq!(fs::read_to_string(fixture.output()).unwrap(), "eof");
}

fn process_stat_field(pid: u32, index: usize) -> String {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let command_end = stat.rfind(')').unwrap();
    stat[command_end + 2..]
        .split_whitespace()
        .nth(index)
        .expect("stat field must exist")
        .into()
}

fn wait_for_session_leader(pid: u32) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let session = process_stat_field(pid, 3);
        if session == pid.to_string() || std::time::Instant::now() >= deadline {
            return session;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
