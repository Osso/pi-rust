#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const BINARY: &str = env!("CARGO_BIN_EXE_run-evals");
const FAKE_CHILD: &str = r#"#!/usr/bin/env python3
import json, os, signal, sys
with open(os.environ['EVAL_RECORD'], 'w') as f:
    json.dump({'args': sys.argv[1:], 'cwd': os.getcwd(),
               'provider': os.getenv('PI_PROVIDER'), 'model': os.getenv('PI_MODEL'),
               'artifacts': os.getenv('PI_EVAL_ARTIFACT_DIR'),
               'inherited': os.getenv('EVAL_INHERITED'),
               'stdin': sys.stdin.read() if os.getenv('EVAL_READ_STDIN') else None}, f)
print('fake stdout', flush=True)
print('fake stderr', file=sys.stderr, flush=True)
if os.getenv('EVAL_SIGNAL'):
    os.kill(os.getpid(), signal.SIGTERM)
sys.exit(int(os.getenv('EVAL_EXIT', '0')))
"#;

struct Fixture {
    temp: TempDir,
    child: PathBuf,
    artifacts: PathBuf,
    record: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let child = temp.path().join("fake-child");
        fs::write(&child, FAKE_CHILD).unwrap();
        fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            child,
            artifacts: temp.path().join("nested/artifacts"),
            record: temp.path().join("record.json"),
            temp,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BINARY);
        command
            .current_dir(self.temp.path())
            .env_remove("PI_PROVIDER")
            .env_remove("PI_MODEL")
            .env_remove("EVAL_EXIT")
            .env_remove("EVAL_SIGNAL")
            .env_remove("EVAL_READ_STDIN")
            .env("PI_EVAL_ARTIFACT_DIR", &self.artifacts)
            .env("PI_EVAL_CHILD_PROGRAM", &self.child)
            .env("EVAL_RECORD", &self.record)
            .env("EVAL_INHERITED", "kept");
        command
    }

    fn record(&self) -> Value {
        serde_json::from_slice(&fs::read(&self.record).unwrap()).unwrap()
    }

    fn assert_validation(&self, output: Output, message: &str) {
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(String::from_utf8(output.stderr).unwrap(), format!("{message}\n"));
        assert!(output.stdout.is_empty());
        assert!(!self.artifacts.exists());
        assert!(!self.record.exists());
    }
}

#[test]
fn default_child_is_cargo_with_native_test_prefix() {
    let fixture = Fixture::new();
    symlink(&fixture.child, fixture.temp.path().join("cargo")).unwrap();
    let path = std::env::join_paths(
        std::iter::once(fixture.temp.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = fixture
        .command()
        .env_remove("PI_EVAL_CHILD_PROGRAM")
        .env("PATH", path)
        .args(["--test", "smoke_eval"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        fixture.record()["args"],
        json!(["test", "-p", "pi-evals", "--test", "smoke_eval"])
    );
}

#[test]
fn pinned_node_script_matches_validation_selection_and_forwarding() {
    let fixture = Fixture::new();
    let reference = fixture.temp.path().join("reference");
    let scripts = reference.join("scripts");
    let vitest = reference.join("node_modules/vitest");
    fs::create_dir_all(&scripts).unwrap();
    fs::create_dir_all(&vitest).unwrap();
    fs::copy(
        "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/evals/scripts/run-evals.mjs",
        scripts.join("run-evals.mjs"),
    )
    .unwrap();
    fs::write(vitest.join("package.json"), r#"{"name":"vitest","version":"0.0.0"}"#).unwrap();
    fs::write(
        vitest.join("vitest.mjs"),
        r#"
import fs from 'node:fs';
fs.writeFileSync(process.env.EVAL_RECORD, JSON.stringify({
    args: process.argv.slice(5), provider: process.env.PI_PROVIDER ?? null,
    model: process.env.PI_MODEL ?? null, artifacts: process.env.PI_EVAL_ARTIFACT_DIR,
}));
console.log('fake stdout');
console.error('fake stderr');
"#,
    )
    .unwrap();
    let cases: &[(&[&str], &str, &str)] = &[
        (&["--provider"], "", ""),
        (&["--model", "-bad"], "", ""),
        (&["--provider", ""], "", ""),
        (&["--model"], "", ""),
        (&["--provider", "-bad"], "", ""),
        (&["--model", ""], "", ""),
        (&["--provider=p"], "fallback", "model"),
        (&["--model=m"], "", ""),
        (&["--provider=", "--model=m"], "", ""),
        (&["--provider=p", "--model="], "", ""),
        (&[], "p", ""),
        (&[], "", "m"),
        (&[], " \u{feff}", "m"),
        (&[], "", ""),
        (&[], " \u{feff}", "\n"),
        (&[], " p ", " m "),
        (&["--provider", " p ", "--model", " m ", "--", "x"], "old", "old"),
        (&["--provider=p", "--model=m", "--test", "case"], "", ""),
        (&["--provider=old", "--provider=-p=q", "--model=-m"], "", ""),
    ];
    for (args, provider, model) in cases {
        let native = fixture
            .command()
            .args(*args)
            .env("PI_PROVIDER", provider)
            .env("PI_MODEL", model)
            .output()
            .unwrap();
        let native_record = native.status.success().then(|| fixture.record());
        let node = Command::new("node")
            .arg(scripts.join("run-evals.mjs"))
            .args(*args)
            .env("PI_PROVIDER", provider)
            .env("PI_MODEL", model)
            .env("PI_EVAL_ARTIFACT_DIR", &fixture.artifacts)
            .env("EVAL_RECORD", &fixture.record)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), node.status.code(), "{args:?}");
        assert_eq!(native.stderr, node.stderr, "{args:?}");
        assert_eq!(native.stdout, node.stdout, "{args:?}");
        if let Some(record) = native_record {
            let expected = fixture.record();
            assert_eq!(
                json!({"args": &record["args"].as_array().unwrap()[3..],
                "provider": record["provider"], "model": record["model"], "artifacts": record["artifacts"]}),
                expected
            );
        }
    }
}

#[test]
fn missing_separate_values_fail_before_side_effects() {
    for flag in ["--provider", "--model"] {
        for suffix in [vec![], vec![""], vec!["-x"], vec!["--model=m"]] {
            let fixture = Fixture::new();
            let output = fixture.command().arg(flag).args(suffix).output().unwrap();
            fixture.assert_validation(output, &format!("Missing value for {flag}"));
        }
    }
}

#[test]
fn incomplete_cli_selection_never_falls_back_to_environment() {
    for args in [
        vec!["--provider=p"],
        vec!["--model=m"],
        vec!["--provider=", "--model=m"],
        vec!["--provider=p", "--model="],
        vec!["--provider", "   ", "--model=m"],
        vec!["--provider=p", "--model", "\u{feff}"],
    ] {
        let fixture = Fixture::new();
        let output = fixture
            .command()
            .args(args)
            .env("PI_PROVIDER", "env-p")
            .env("PI_MODEL", "env-m")
            .output()
            .unwrap();
        fixture.assert_validation(output, "CLI model selection requires both --provider and --model.");
    }
}

#[test]
fn incomplete_environment_selection_fails() {
    for (provider, model) in [("p", ""), ("", "m"), ("  ", "m"), ("p", "\u{feff}")] {
        let fixture = Fixture::new();
        let output = fixture
            .command()
            .env("PI_PROVIDER", provider)
            .env("PI_MODEL", model)
            .output()
            .unwrap();
        fixture.assert_validation(
            output,
            "Default model selection requires both PI_PROVIDER and PI_MODEL.",
        );
    }
}

#[test]
fn separate_and_equals_forms_trim_override_and_forward() {
    for args in [
        vec!["--provider", "\u{feff}p ", "--model", " m\n"],
        vec!["--provider=\u{feff}p ", "--model= m\n"],
        vec!["--provider=old", "--model=old", "--provider=p", "--model=m"],
    ] {
        let fixture = Fixture::new();
        let output = fixture
            .command()
            .args(["--test", "smoke_eval"])
            .args(args)
            .args(["--", "--ignored", "two words", ""])
            .env("PI_PROVIDER", "env-p")
            .env("PI_MODEL", "env-m")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            fixture.record(),
            json!({
                "args": ["test", "-p", "pi-evals", "--test", "smoke_eval", "--", "--ignored", "two words", ""],
                "cwd": ROOT, "provider": "p", "model": "m",
                "artifacts": fixture.artifacts, "inherited": "kept", "stdin": null,
            })
        );
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("[eval] default-model=p/m\n")
        );
    }
}

#[test]
fn equals_values_can_start_with_dash_or_contain_equals() {
    let fixture = Fixture::new();
    assert!(
        fixture
            .command()
            .args(["--provider=-p=q", "--model=-m"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fixture.record()["provider"], "-p=q");
    assert_eq!(fixture.record()["model"], "-m");
}

#[test]
fn environment_fallback_trims_both_values() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .env("PI_PROVIDER", " \u{feff}p\t")
        .env("PI_MODEL", "\nm ")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fixture.record()["provider"], "p");
    assert_eq!(fixture.record()["model"], "m");
}

#[test]
fn absent_or_blank_selection_removes_child_environment() {
    for selection in [None, Some((" \u{feff}", "\t\n"))] {
        let fixture = Fixture::new();
        let mut command = fixture.command();
        if let Some((provider, model)) = selection {
            command.env("PI_PROVIDER", provider).env("PI_MODEL", model);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(fixture.record()["provider"], Value::Null);
        assert_eq!(fixture.record()["model"], Value::Null);
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("[eval] default-model=none\n")
        );
    }
}

#[test]
fn recursive_private_artifacts_and_ordered_stderr_precede_child_output() {
    let fixture = Fixture::new();
    let output = fixture.command().output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::metadata(&fixture.artifacts).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(fixture.artifacts.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "[eval] default-model=none\n[eval] artifacts={}\nfake stderr\n",
            fixture.artifacts.display()
        )
    );
    assert_eq!(output.stdout, b"fake stdout\n");
}

#[test]
fn relative_artifacts_resolve_lexically_against_package_not_launch_directory() {
    let fixture = Fixture::new();
    let relative = Path::new(ROOT).strip_prefix("/").unwrap();
    // Traversal from the package root reaches the fixture without writing in the checkout.
    let parents: PathBuf = relative.components().map(|_| "..").collect();
    let path = parents
        .join(fixture.temp.path().strip_prefix("/").unwrap())
        .join("gone/../artifacts");
    let output = fixture.command().env("PI_EVAL_ARTIFACT_DIR", path).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        fixture.record()["artifacts"],
        json!(fixture.temp.path().join("artifacts"))
    );
    assert!(!fixture.temp.path().join("gone").exists());
}

#[test]
fn absolute_artifacts_do_not_canonicalize_symlinks_or_change_existing_permissions() {
    let fixture = Fixture::new();
    let target = fixture.temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    let alias = fixture.temp.path().join("alias");
    symlink(&target, &alias).unwrap();
    assert!(
        fixture
            .command()
            .env("PI_EVAL_ARTIFACT_DIR", &alias)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fixture.record()["artifacts"], json!(alias));
    assert_eq!(fs::metadata(target).unwrap().permissions().mode() & 0o777, 0o755);
}

#[test]
fn default_artifacts_use_iso_timestamp_and_unique_uuid() {
    for empty in [false, true] {
        let fixture = Fixture::new();
        // The Cargo runner mounts the checkout read-only. Overlay only this child's
        // package root with a writable fixture; keep the enclosing sandbox intact.
        let template = fixture.command();
        let mut command = Command::new("bwrap");
        command
            .args(["--ro-bind", "/", "/", "--bind"])
            .arg(fixture.temp.path())
            .arg(ROOT)
            .arg("--")
            .arg(BINARY);
        for (key, value) in template.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
        if empty {
            command.env("PI_EVAL_ARTIFACT_DIR", "");
        } else {
            command.env_remove("PI_EVAL_ARTIFACT_DIR");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let record = fixture.record();
        let path = Path::new(record["artifacts"].as_str().unwrap());
        assert_eq!(path.parent().unwrap(), Path::new(ROOT).join(".eval"));
        let name = path.file_name().unwrap().to_str().unwrap();
        let (timestamp, uuid) = name.split_once('_').unwrap();
        assert!(
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}\.\d{3}Z$")
                .unwrap()
                .is_match(timestamp)
        );
        assert!(
            regex::Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
                .unwrap()
                .is_match(uuid)
        );
        let created = fixture.temp.path().join(".eval").join(name);
        assert_eq!(fs::metadata(&created).unwrap().permissions().mode() & 0o777, 0o700);
        fs::remove_dir(created).unwrap();
    }
}

#[test]
fn child_exit_codes_and_signal_are_propagated() {
    for (exit, signal, expected) in [("0", false, 0), ("3", false, 3), ("0", true, 1)] {
        let fixture = Fixture::new();
        let output = fixture
            .command()
            .env("EVAL_EXIT", exit)
            .env("EVAL_SIGNAL", if signal { "yes" } else { "" })
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected));
        assert!(fixture.record.exists());
    }
}

#[test]
fn child_inherits_stdin() {
    let fixture = Fixture::new();
    let input = fixture.temp.path().join("input");
    fs::write(&input, "inherited input\n").unwrap();
    let output = fixture
        .command()
        .env("EVAL_READ_STDIN", "yes")
        .stdin(fs::File::open(input).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fixture.record()["stdin"], "inherited input\n");
}

#[test]
fn spawn_failure_reports_error_and_nonzero_status() {
    let fixture = Fixture::new();
    let missing = fixture.temp.path().join("missing-child");
    let output = fixture
        .command()
        .env("PI_EVAL_CHILD_PROGRAM", &missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("[eval] default-model=none\n[eval] artifacts="));
    assert!(stderr.contains("No such file or directory"), "{stderr}");
    assert!(stderr.contains(missing.to_str().unwrap()), "{stderr}");
    assert!(!fixture.record.exists());
}

#[test]
fn mkdir_failure_reports_error_without_announcing_or_spawning() {
    let fixture = Fixture::new();
    fs::write(&fixture.artifacts.parent().unwrap().to_path_buf(), "not a directory").unwrap();
    let output = fixture.command().output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("[eval]"));
    assert!(stderr.contains(fixture.artifacts.to_str().unwrap()), "{stderr}");
    assert!(!fixture.record.exists());
}
