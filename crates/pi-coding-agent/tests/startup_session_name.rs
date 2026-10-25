//! Startup `--name` writes session metadata only
//! (`test/startup-session-name.test.ts`).

use pi_js_compat::js_json::JsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use pi_coding_agent::core::session_control_db::{get_control_db_path, read_session_metadata};
use serde_json::{Value, json};

struct CliDirs {
    _temp: tempfile::TempDir,
    agent_dir: PathBuf,
    project_dir: PathBuf,
    session_file: PathBuf,
    state_dir: PathBuf,
}

struct CliResult {
    code: i32,
    stderr: String,
}

fn setup() -> CliDirs {
    let temp = tempfile::tempdir().expect("temp dir");
    let agent_dir = temp.path().join("agent");
    let project_dir = temp.path().join("project");
    let state_dir = temp.path().join("state");
    let session_file = temp.path().join("session.jsonl");
    for directory in [&agent_dir, &project_dir, &state_dir] {
        std::fs::create_dir_all(directory).expect("fixture directory");
    }
    let lines = [
        json!({
            "type": "session",
            "version": 3,
            "id": "startup-session",
            "timestamp": "2026-08-01T00:00:00.000Z",
            "cwd": project_dir.display().to_string(),
        }),
        json!({
            "type": "message",
            "id": "existing",
            "parentId": null,
            "timestamp": "2026-08-01T00:00:01.000Z",
            "message": {
                "role": "user",
                "content": "existing prompt",
                "timestamp": 1,
            },
        }),
    ];
    std::fs::write(
        &session_file,
        format!(
            "{}\n",
            lines.iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n")
        ),
    )
    .expect("session file");
    CliDirs {
        _temp: temp,
        agent_dir,
        project_dir,
        session_file,
        state_dir,
    }
}

fn run_cli(args: &[&str], dirs: &CliDirs) -> CliResult {
    let output = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(args)
        .current_dir(&dirs.project_dir)
        .env("PI_CODING_AGENT_DIR", &dirs.agent_dir)
        .env("PI_CODING_AGENT_STATE_DIR", &dirs.state_dir)
        .env("PI_OFFLINE", "1")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("process must run");
    CliResult {
        code: output.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn session_entry_types(session_file: &Path) -> Vec<String> {
    std::fs::read_to_string(session_file)
        .expect("session file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line).expect("jsonl line")["type"]
                .as_str()
                .expect("entry type")
                .to_owned()
        })
        .collect()
}

#[test]
fn startup_name_writes_session_metadata_only() {
    let dirs = setup();
    let session = dirs.session_file.display().to_string();
    let result = run_cli(
        &[
            "--session",
            &session,
            "--name",
            "  CLI Named Session  ",
            "--model",
            "missing-model",
            "-p",
            "hi",
        ],
        &dirs,
    );

    assert_eq!(result.code, 1);
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path(dirs.state_dir.as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    assert_eq!(
        read_session_metadata(&control_db_path, &session)
            .expect("metadata")
            .expect("session metadata")
            .name
            .as_ref(),
        Some(&JsString::from("CLI Named Session"))
    );
    assert!(
        !session_entry_types(&dirs.session_file)
            .iter()
            .any(|kind| kind == "session_info"),
        "the CLI never appends session_info entries"
    );
}

#[test]
fn empty_startup_name_is_rejected_without_touching_metadata() {
    let dirs = setup();
    let session = dirs.session_file.display().to_string();
    let result = run_cli(
        &[
            "--session",
            &session,
            "--name",
            "   ",
            "--model",
            "missing-model",
            "-p",
            "hi",
        ],
        &dirs,
    );

    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("--name requires a non-empty value"));
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path(dirs.state_dir.as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    assert_eq!(
        read_session_metadata(&control_db_path, &session)
            .expect("metadata")
            .and_then(|metadata| metadata.name),
        None
    );
    assert!(
        !session_entry_types(&dirs.session_file)
            .iter()
            .any(|kind| kind == "session_info"),
        "the rejected startup name leaves no session_info entry"
    );
}
