//! Differential test for `packages/ai/src/cli.ts` against the `pi-ai` binary.
//!
//! Every case is network-free: help/list/unknown-command and login flows that
//! stop at an invalid selection before any OAuth request is issued.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));
const TIMEOUT: Duration = Duration::from_secs(60);

struct Output {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct Case {
    name: &'static str,
    args: &'static [&'static str],
    stdin: &'static str,
}

const CASES: [Case; 15] = [
    Case {
        name: "no args",
        args: &[],
        stdin: "",
    },
    Case {
        name: "help",
        args: &["help"],
        stdin: "",
    },
    Case {
        name: "--help",
        args: &["--help"],
        stdin: "",
    },
    Case {
        name: "-h",
        args: &["-h"],
        stdin: "",
    },
    Case {
        name: "list",
        args: &["list"],
        stdin: "",
    },
    Case {
        name: "list ignores extra args",
        args: &["list", "extra"],
        stdin: "",
    },
    Case {
        name: "unknown command",
        args: &["bogus"],
        stdin: "",
    },
    Case {
        name: "login unknown provider",
        args: &["login", "unknown-provider"],
        stdin: "",
    },
    Case {
        name: "login provider without oauth",
        args: &["login", "openai"],
        stdin: "",
    },
    Case {
        name: "login selection out of range",
        args: &["login"],
        stdin: "999\n",
    },
    Case {
        name: "login selection not a number",
        args: &["login"],
        stdin: "abc\n",
    },
    Case {
        name: "login selection empty",
        args: &["login"],
        stdin: "\n",
    },
    Case {
        name: "codex selection out of range",
        args: &["login", "openai-codex"],
        stdin: "9\n",
    },
    Case {
        name: "codex selection zero",
        args: &["login", "openai-codex"],
        stdin: "0\n",
    },
    Case {
        name: "codex selection blank",
        args: &["login", "openai-codex"],
        stdin: "\n",
    },
];

fn source_worktree() -> PathBuf {
    let lock: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock JSON");
    let worktree = PathBuf::from(lock["worktree"].as_str().expect("worktree"));
    let commit = lock["commit"].as_str().expect("commit");
    let head = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    assert!(head.status.success(), "git rev-parse failed");
    assert_eq!(
        String::from_utf8(head.stdout).expect("utf8").trim(),
        commit,
        "source worktree must match docs/porting/source-lock.json"
    );
    worktree
}

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pi-ai-cli-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp dir");
    path
}

fn run(mut command: Command, cwd: &Path, stdin: &str) -> Output {
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn cli");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    wait_with_timeout(&mut child)
}

fn wait_with_timeout(child: &mut Child) -> Output {
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        io::copy(&mut { stdout }, &mut buffer).expect("read stdout");
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        io::copy(&mut { stderr }, &mut buffer).expect("read stderr");
        buffer
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("cli did not exit within {TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    Output {
        status,
        stdout: stdout_reader.join().expect("stdout thread"),
        stderr: stderr_reader.join().expect("stderr thread"),
    }
}

#[test]
fn cli_output_matches_the_pinned_typescript_cli() {
    let worktree = source_worktree();
    let tsx = worktree.join("node_modules/.bin/tsx");
    let tsconfig = worktree.join("tsconfig.json");
    let cli = worktree.join("packages/ai/src/cli.ts");

    for case in &CASES {
        let mut typescript = Command::new(&tsx);
        typescript.arg("--tsconfig").arg(&tsconfig).arg(&cli);
        typescript.args(case.args);
        let ts_dir = temp_dir("ts");
        let ts_output = run(typescript, &ts_dir, case.stdin);

        let mut rust = Command::new(env!("CARGO_BIN_EXE_pi-ai"));
        rust.args(case.args);
        let rust_dir = temp_dir("rust");
        let rust_output = run(rust, &rust_dir, case.stdin);

        assert_eq!(
            ts_output.status.code(),
            rust_output.status.code(),
            "{}: exit code",
            case.name
        );
        assert_eq!(
            String::from_utf8_lossy(&ts_output.stdout),
            String::from_utf8_lossy(&rust_output.stdout),
            "{}: stdout",
            case.name
        );
        assert_eq!(
            String::from_utf8_lossy(&ts_output.stderr),
            String::from_utf8_lossy(&rust_output.stderr),
            "{}: stderr",
            case.name
        );

        let _ = fs::remove_dir_all(&ts_dir);
        let _ = fs::remove_dir_all(&rust_dir);
    }
}
