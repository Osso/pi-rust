//! Real pty proof of frozen config dispatch, including arguments that it ignores.
#[path = "support/pty.rs"]
mod pty;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

struct Fixture {
    directory: tempfile::TempDir,
    source: PathBuf,
    environment: Vec<(String, String)>,
}

fn fixture() -> Fixture {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = PathBuf::from(lock["worktree"].as_str().unwrap());
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        lock["commit"].as_str().unwrap()
    );
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("package");
    for name in [
        "work",
        "home",
        "agent",
        "state",
        "package/theme",
        "package/src/modes/interactive/theme",
    ] {
        std::fs::create_dir_all(directory.path().join(name)).unwrap();
    }
    std::fs::copy(
        source.join("packages/coding-agent/package.json"),
        package.join("package.json"),
    )
    .unwrap();
    for name in ["dark.json", "light.json"] {
        let theme = source
            .join("packages/coding-agent/src/modes/interactive/theme")
            .join(name);
        std::fs::copy(&theme, package.join("theme").join(name)).unwrap();
        std::fs::copy(&theme, package.join("src/modes/interactive/theme").join(name)).unwrap();
    }
    let environment = [
        ("HOME", directory.path().join("home").display().to_string()),
        (
            "PI_CODING_AGENT_DIR",
            directory.path().join("agent").display().to_string(),
        ),
        (
            "PI_CODING_AGENT_STATE_DIR",
            directory.path().join("state").display().to_string(),
        ),
        ("PI_PACKAGE_DIR", package.display().to_string()),
        ("PI_OFFLINE", "1".to_owned()),
        ("NO_COLOR", "1".to_owned()),
        ("TERM", "xterm-256color".to_owned()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    Fixture {
        directory,
        source,
        environment,
    }
}

fn run_config(fixture: &Fixture, args: &[&str], frozen: bool) -> Value {
    let oracle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/config-dispatch-process.mts");
    let module = fixture.source.join("packages/coding-agent/src/package-manager-cli.ts");
    let tsx = fixture.source.join("node_modules/.bin/tsx");
    let tsconfig = fixture.source.join("tsconfig.json");
    let mut source_args = vec![
        "--tsconfig",
        tsconfig.to_str().unwrap(),
        oracle.to_str().unwrap(),
        module.to_str().unwrap(),
    ];
    source_args.extend_from_slice(args);
    let binary = if frozen {
        tsx.as_path()
    } else {
        Path::new(env!("CARGO_BIN_EXE_pi"))
    };
    let process_args = if frozen { source_args.as_slice() } else { args };
    let mut child = pty::spawn(
        binary,
        process_args,
        &fixture.environment,
        &fixture.directory.path().join("work"),
    );
    eprintln!("config dispatch frozen={frozen} args={args:?} pid={}", child.id());
    assert!(child.id() > 0);
    let initial_termios = child.initial_termios();
    child.wait_for("Resource Configuration", Duration::from_secs(30));
    let initial_output = child.output();
    child.write_input("\u{1b}");
    let code = child.wait(Duration::from_secs(10));
    assert_eq!(child.termios(), initial_termios, "selector restores terminal modes");
    child.close_master();
    // The dispatch contract is selector entry and normal exit, not a whole-TUI
    // escape-byte comparison (the component has its separate frozen proofs).
    json!({
        "selector": initial_output.contains("Resource Configuration"),
        "filter": initial_output.contains("Type to filter resources"),
        "help": initial_output.contains("Usage:"),
        "invalidOption": initial_output.contains("Unknown option"),
        "invalidArgument": initial_output.contains("Unexpected argument"),
        "exitCode": code,
    })
}

#[test]
fn config_ignored_arguments_enter_the_same_selector_and_exit_zero_as_frozen() {
    let expected =
        json!({"selector":true,"filter":true,"help":false,"invalidOption":false,"invalidArgument":false,"exitCode":0});
    for args in [
        vec!["config", "--help"],
        vec!["config", "--bogus"],
        vec!["config", "extra"],
        vec!["config", "-l"],
        vec!["config", "--local", "--help", "extra"],
    ] {
        let fixture = fixture();
        let frozen = run_config(&fixture, &args, true);
        assert_eq!(frozen, expected, "frozen {args:?}");
        assert_eq!(run_config(&fixture, &args, false), frozen, "native {args:?}");
    }
}
