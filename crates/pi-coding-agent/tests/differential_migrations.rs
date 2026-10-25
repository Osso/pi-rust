//! Differential test for `migrations.ts` against fixture agent directories.

use crate::shared_oracle as oracle;

use std::fs;
use std::path::{Path, PathBuf};

use oracle::TsOracle;
use std::cell::RefCell;

use pi_coding_agent::migrations::{MigrationOutput, run_migrations_with};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use tempfile::TempDir;

#[derive(Default)]
struct RecordingOutput(RefCell<Vec<String>>);

impl MigrationOutput for RecordingOutput {
    fn line(&self, text: &JsString) {
        self.0.borrow_mut().push(text.to_utf8_lossy());
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn snapshot(root: &Path) -> Vec<Value> {
    let mut entries = Vec::new();
    collect_entries(root, root, &mut entries);
    entries
}

fn collect_entries(root: &Path, dir: &Path, entries: &mut Vec<Value>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read_dir.filter_map(Result::ok).map(|entry| entry.path()).collect();
    // Directory entry order is filesystem-dependent; snapshots are compared sorted.
    paths.sort();
    for path in paths {
        let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let metadata = fs::metadata(&path).unwrap();
        if metadata.is_dir() {
            entries.push(json!({"path": relative, "kind": "dir"}));
            collect_entries(root, &path, entries);
        } else if metadata.is_file() {
            entries.push(json!({
                "path": relative,
                "kind": "file",
                "contents": pi_js_compat::base64::buffer_to_base64(&fs::read(&path).unwrap()),
            }));
        }
    }
}

fn run_case(name: &str, files: &[(&str, &str)]) {
    let temp = TempDir::new().unwrap();
    let oracle_agent = temp.path().join("oracle-agent");
    let oracle_cwd = temp.path().join("oracle-project");
    let rust_agent = temp.path().join("rust-agent");
    let rust_cwd = temp.path().join("rust-project");
    for dir in [&oracle_agent, &oracle_cwd, &rust_agent, &rust_cwd] {
        fs::create_dir_all(dir).unwrap();
    }
    for (relative, contents) in files {
        write(&oracle_agent, relative, contents);
        write(&rust_agent, relative, contents);
    }

    let mut oracle = TsOracle::spawn_with_environment(
        "migrations-oracle.ts",
        "packages/coding-agent/src/migrations.ts",
        &[("PI_CODING_AGENT_DIR", oracle_agent.clone().into_os_string())],
    );
    let response = oracle.request(&json!({
        "id": name,
        "cwd": oracle_cwd.to_string_lossy(),
        "roots": [oracle_agent.to_string_lossy(), oracle_cwd.join(".pi").to_string_lossy()],
    }));
    assert_eq!(response["ok"], true, "oracle failed for {name}: {response}");

    let output_sink = RecordingOutput::default();
    let result = run_migrations_with(
        &rust_agent,
        &rust_agent.join("bin"),
        &JsString::from(rust_cwd.to_str().unwrap()),
        &output_sink,
    );
    let output = &response["output"];
    assert_eq!(
        output["migratedAuthProviders"],
        json!(result.migrated_auth_providers),
        "provider list differs for {name}"
    );
    assert_eq!(
        output["deprecationWarnings"],
        json!(result.deprecation_warnings),
        "warnings differ for {name}"
    );
    assert_eq!(
        output["consoleLog"],
        json!(output_sink.0.borrow().clone()),
        "console output differs for {name}"
    );
    assert_eq!(
        output["trees"][0]["entries"],
        json!(snapshot(&rust_agent)),
        "agent tree differs for {name}"
    );
    assert_eq!(
        output["trees"][1]["entries"],
        json!(snapshot(&rust_cwd.join(".pi"))),
        "project tree differs for {name}"
    );
}

#[test]
fn differential_empty_agent_directory() {
    run_case("empty", &[]);
}

#[test]
fn differential_oauth_and_settings_api_keys() {
    run_case(
        "auth",
        &[
            ("oauth.json", r#"{"anthropic":{"refresh":"r1","access":"a1"},"bad":7}"#),
            (
                "settings.json",
                r#"{"theme":"dark","apiKeys":{"openai":"sk-1","skipped":false},"packages":["npm:x"]}"#,
            ),
        ],
    );
}

#[test]
fn differential_existing_auth_wins() {
    run_case(
        "existing-auth",
        &[
            ("auth.json", r#"{"anthropic":{"type":"api_key","key":"keep"}}"#),
            (
                "oauth.json",
                r#"{"anthropic":{"refresh":"n"},"openai":{"refresh":"o"}}"#,
            ),
        ],
    );
}

#[test]
fn differential_invalid_auth_json() {
    run_case(
        "invalid-auth",
        &[
            ("auth.json", "{broken"),
            ("oauth.json", r#"{"openai":{"refresh":"r"}}"#),
        ],
    );
}

#[test]
fn differential_agent_root_sessions() {
    run_case(
        "sessions",
        &[
            (
                "2026-01-01T00-00-00.jsonl",
                "{\"type\":\"session\",\"cwd\":\"/work/app\",\"version\":3}\n{\"type\":\"message\"}\n",
            ),
            ("not-session.jsonl", "{\"type\":\"message\"}\n"),
            ("blank.jsonl", "\n"),
            ("nested/data.jsonl", "{\"type\":\"session\",\"cwd\":\"/nested\"}\n"),
        ],
    );
}

#[test]
fn differential_tools_to_bin() {
    run_case(
        "tools",
        &[
            ("tools/fd", "fd"),
            ("tools/rg", "rg"),
            ("tools/fd.exe", "fdw"),
            ("tools/keep.txt", "k"),
        ],
    );
}

#[test]
fn differential_keybindings_migration() {
    run_case(
        "keybindings",
        &[(
            "keybindings.json",
            r#"{"submit":"enter","cursorUp":["k","up"],"tui.editor.cursorDown":"j","custom.binding":"x"}"#,
        )],
    );
}

#[test]
fn differential_commands_to_prompts_and_warnings() {
    run_case(
        "commands",
        &[
            ("commands/a.md", "a"),
            ("hooks/h.ts", "h"),
            ("tools/custom.ts", "c"),
            ("tools/.DS_Store", "x"),
        ],
    );
}
