//! Node/libuv errno codes for spawn and filesystem failures.
//!
//! Every caller that formats a failed spawn or `fs` operation must report the
//! real `uv_err_name` code (`ETXTBSY`, `ENOTDIR`, ...) instead of collapsing
//! unknown codes to `EIO`. The errors are produced at the real OS boundary:
//! `execve` returns `ETXTBSY`/`EACCES`/`ENOENT`/`ENOTDIR` for these fixtures.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use pi_coding_agent::bundled_extensions::bwrap::backend::run_bwrap_availability_check;
use pi_coding_agent::bundled_extensions::pyrun::runner::{
    CanonicalPyrunEvalParams, PyrunRunnerClient, PyrunRunnerOptions,
};
use pi_coding_agent::core::tools::node_fs::node_fs_error;
use pi_coding_agent::utils::child_process::{node_error_code, node_spawn_error_message};

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

fn script(dir: &Path, name: &str, mode: u32) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "#!/bin/sh\nexit 0\n").expect("script");
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("script mode");
    path
}

fn spawn_error(path: &Path) -> std::io::Error {
    Command::new(path).spawn().expect_err("spawn must fail")
}

#[test]
fn spawn_error_message_preserves_unpaired_command_units() {
    use pi_js_compat::js_json::{JsString, encode_node_utf8};
    let directory = temp_dir();
    let command = JsString::from(directory.path().to_str().unwrap()) + "/" + JsString::from_utf16(vec![0xd800]);
    let error = Command::new(encode_node_utf8(&command))
        .spawn()
        .expect_err("missing executable");
    assert_eq!(
        node_spawn_error_message("spawn", &command, &error),
        JsString::from("spawn ") + command + " ENOENT"
    );
}

#[test]
fn reports_enoent_for_a_missing_executable() {
    let dir = temp_dir();
    let missing = dir.path().join("missing");
    let error = spawn_error(&missing);

    assert_eq!(error.raw_os_error(), Some(2));
    assert_eq!(node_error_code(&error), "ENOENT");
    assert_eq!(
        node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("missing"), &error),
        "spawn missing ENOENT"
    );
}

#[test]
fn reports_eacces_for_a_non_executable_file() {
    let dir = temp_dir();
    let path = script(dir.path(), "not-executable", 0o644);
    let error = spawn_error(&path);

    assert_eq!(error.raw_os_error(), Some(13));
    assert_eq!(node_error_code(&error), "EACCES");
    assert_eq!(
        node_spawn_error_message(
            "spawn",
            &pi_js_compat::js_json::JsString::from("not-executable"),
            &error
        ),
        "spawn not-executable EACCES"
    );
}

#[test]
fn reports_etxtbsy_while_the_script_is_open_for_writing() {
    let dir = temp_dir();
    let path = script(dir.path(), "busy", 0o755);
    let _open_for_writing = fs::OpenOptions::new().write(true).open(&path).expect("open for write");
    let error = spawn_error(&path);

    assert_eq!(error.raw_os_error(), Some(26));
    assert_eq!(node_error_code(&error), "ETXTBSY");
    assert_eq!(
        node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("busy"), &error),
        "spawn busy ETXTBSY"
    );
    assert!(node_fs_error(&error, "open", &JsString::from("busy")).starts_with("ETXTBSY: "));
}

#[test]
fn reports_enotdir_when_a_path_component_is_a_file() {
    let dir = temp_dir();
    let file = dir.path().join("plain-file");
    fs::write(&file, "not a directory").expect("file");
    let error = spawn_error(&file.join("child"));

    assert_eq!(error.raw_os_error(), Some(20));
    assert_eq!(node_error_code(&error), "ENOTDIR");
    assert_eq!(
        node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("child"), &error),
        "spawn child ENOTDIR"
    );
    assert!(node_fs_error(&error, "open", &JsString::from("child")).starts_with("ENOTDIR: "));
}

#[test]
fn reports_etxtbsy_for_the_bwrap_availability_check() {
    let dir = temp_dir();
    let path = script(dir.path(), "bwrap", 0o755);
    let _open_for_writing = fs::OpenOptions::new().write(true).open(&path).expect("open for write");

    let error =
        run_bwrap_availability_check(&JsString::from(&path.display().to_string())).expect_err("bwrap must fail");
    assert_eq!(
        error,
        format!(
            "bubblewrap is required for sandbox profile but is unavailable: spawnSync {} ETXTBSY",
            path.display()
        )
    );
}

#[tokio::test]
async fn reports_etxtbsy_for_the_pyrun_runner_spawn() {
    let dir = temp_dir();
    let path = script(dir.path(), "fake-pyrun", 0o755);
    let _open_for_writing = fs::OpenOptions::new().write(true).open(&path).expect("open for write");
    let runner = PyrunRunnerClient::new(PyrunRunnerOptions {
        command: Some(JsString::from(path.display().to_string())),
        args: Some(Vec::new()),
        detached: Some(false),
        ..Default::default()
    });

    let error = runner
        .evaluate(
            CanonicalPyrunEvalParams {
                code: JsString::from("print(1)"),
                pi: None,
                pi_bridge: Some(false),
                session_id: None,
                stream_console: Some(true),
            },
            None,
            None,
            None,
        )
        .await
        .expect_err("spawn must fail");
    assert_eq!(error.to_string(), format!("spawn {} ETXTBSY", path.display()));
}
