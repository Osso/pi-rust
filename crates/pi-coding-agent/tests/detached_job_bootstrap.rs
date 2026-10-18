use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::detached_job_bootstrap::{
    DetachedPayloadError, GatedDetachedPayloadInput, spawn_gated_detached_payload,
};

struct Fixture {
    directory: tempfile::TempDir,
    output: PathBuf,
    identity: PathBuf,
    marker: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("job.output");
        let identity = directory.path().join("payload.json");
        let marker = directory.path().join("payload-ran");
        Self {
            directory,
            output,
            identity,
            marker,
        }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn output(&self) -> &Path {
        &self.output
    }

    fn identity(&self) -> &Path {
        &self.identity
    }

    fn marker(&self) -> &Path {
        &self.marker
    }
}

fn payload_input<'a>(fixture: &'a Fixture, args: &'a [String]) -> GatedDetachedPayloadInput<'a> {
    let _ = fixture;
    GatedDetachedPayloadInput {
        args,
        command: "/bin/sh",
        cwd: fixture.root(),
        env: None,
        identity_path: fixture.identity(),
        stderr_path: fixture.output(),
        stdout_path: fixture.output(),
    }
}

fn payload_args(fixture: &Fixture) -> Vec<String> {
    vec![
        "-c".to_owned(),
        format!("printf ran > '{}'; printf output", fixture.marker().to_string_lossy()),
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
    let stderr = fixture.root().join("stderr.output");
    let mut input = payload_input(&fixture, &args);
    input.stderr_path = stderr.as_path();
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
async fn reports_a_missing_command() {
    let fixture = Fixture::new();
    let args: Vec<String> = Vec::new();
    let mut input = payload_input(&fixture, &args);
    input.command = "/nonexistent/pi-detached";
    assert_eq!(
        spawn_gated_detached_payload(&input)
            .expect_err("missing command must fail")
            .to_string(),
        "Could not start detached payload: /nonexistent/pi-detached"
    );
}

#[tokio::test]
async fn gives_the_payload_a_closed_stdin_after_release() {
    let fixture = Fixture::new();
    let args = vec!["-c".to_owned(), "read line || printf eof".to_owned()];
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
        .to_owned()
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
