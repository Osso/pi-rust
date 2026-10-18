use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

use pi_agent_core::unicode::{normalize_nfc, normalize_nfd, normalize_nfkc, normalize_nfkd};

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

struct Oracle {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Oracle {
    fn spawn() -> Self {
        let source_lock = read_source_lock();
        verify_source_head(&source_lock);
        let mut child = spawn_oracle_process(&source_lock);
        let stdin = BufWriter::new(child.stdin.take().expect("oracle stdin must be piped"));
        let stdout = BufReader::new(child.stdout.take().expect("oracle stdout must be piped"));
        Self { child, stdin, stdout }
    }

    fn request(&mut self, request: &Value) -> Value {
        serde_json::to_writer(&mut self.stdin, request).expect("oracle request must serialize");
        self.stdin
            .write_all(b"\n")
            .expect("oracle request must be newline-delimited");
        self.stdin.flush().expect("oracle stdin must flush");
        let mut line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut line)
            .expect("oracle response must be readable");
        assert_ne!(bytes_read, 0, "oracle must return one response per request");
        serde_json::from_str(&line).expect("oracle response must be valid JSON")
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    let worktree = value["worktree"]
        .as_str()
        .map(PathBuf::from)
        .expect("source lock worktree must be a string");
    let commit = value["commit"]
        .as_str()
        .map(str::to_owned)
        .expect("source lock commit must be a string");
    SourceLock { worktree, commit }
}

fn spawn_oracle_process(source_lock: &SourceLock) -> Child {
    let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
    let tsconfig = source_lock.worktree.join("tsconfig.json");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/unicode-normalize-oracle.ts");
    let mut command = Command::new(&tsx);
    command
        .arg("--tsconfig")
        .arg(tsconfig)
        .arg(oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    command
        .spawn()
        .expect("worktree-local tsx must start the Unicode normalization oracle")
}

fn verify_source_head(source_lock: &SourceLock) {
    let output = Command::new("git")
        .arg("-C")
        .arg(&source_lock.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_commit = String::from_utf8(output.stdout)
        .expect("git HEAD must be UTF-8")
        .trim()
        .to_owned();
    assert_eq!(
        actual_commit, source_lock.commit,
        "TypeScript source HEAD must equal docs/porting/source-lock.json"
    );
}

fn corpus() -> Vec<&'static str> {
    vec![
        "plain ASCII",
        "한글",
        "한글",
        "한글 mixed with 한",
        "e\u{0301}",
        "A\u{030A}\u{0323}",
        "e\u{0301}\u{0323}",
        "Å",
        "Å",
        "ﬁ",
        "ﬂ",
        "①",
        "Ⅻ",
        "²",
        "ＡＢＣ",
        "ｶﾀｶﾅ",
        "㍿",
        "㈱",
        "豈",
        "ﷺ",
        "𝐀",
        "𝕏",
        "क्\u{0937}",
        "क्ष",
        "ά",
        "α\u{0301}",
        "ΐ",
        "ế",
        "e\u{0302}\u{0301}",
        "İ",
        "ı",
        "👨‍👩‍👧‍👦",
        "🏳️‍🌈",
        "🇺🇸",
        "👍🏽",
        "©️",
        "❤️",
        "\u{00A0}",
        "\u{202F}",
        "\u{FEFF}",
        "\u{0085}",
        "\u{200B}",
        "a\u{FEFF}b",
        "line\u{2028}separator",
        "line\u{2029}separator",
        "\u{1D400}\u{1D401}\u{1D402}",
        "e\u{0301} ﬁ ① Ａ 한글 👨‍👩‍👧‍👦 \u{00A0}\u{FEFF}",
    ]
}

#[test]
fn normalization_forms_match_node_icu() {
    let mut oracle = Oracle::spawn();
    for (index, text) in corpus().iter().enumerate() {
        let response = oracle.request(&json!({"id": index, "text": text}));
        assert_eq!(response["id"], json!(index), "oracle response id");
        let expected = &response["result"];
        assert_eq!(
            normalize_nfc(text),
            expected["nfc"].as_str().expect("NFC string"),
            "NFC case {index}: {text:?}"
        );
        assert_eq!(
            normalize_nfd(text),
            expected["nfd"].as_str().expect("NFD string"),
            "NFD case {index}: {text:?}"
        );
        assert_eq!(
            normalize_nfkc(text),
            expected["nfkc"].as_str().expect("NFKC string"),
            "NFKC case {index}: {text:?}"
        );
        assert_eq!(
            normalize_nfkd(text),
            expected["nfkd"].as_str().expect("NFKD string"),
            "NFKD case {index}: {text:?}"
        );
    }
}
