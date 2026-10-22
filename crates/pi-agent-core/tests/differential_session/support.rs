use pi_js_compat::js_json::JsString;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use pi_agent_core::harness::session::{
    BranchBounds, BranchQuery, Entry, EntryCursor, EntryOrder, EntryQuery, EntryType, JsonlSessionMetadata, LaneRecord,
    RecordQuery, RecordType, SessionError, SessionMetadata,
};
use serde_json::{Value, json};

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

#[derive(Debug)]
struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

pub(crate) struct TsOracle {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl TsOracle {
    pub(crate) fn spawn(oracle_filename: &str, source_module: &str) -> Self {
        let source_lock = read_source_lock();
        verify_source_head(&source_lock);
        let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
        let tsconfig = source_lock.worktree.join("tsconfig.json");
        let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle")
            .join(oracle_filename);
        let mut command = Command::new(&tsx);
        command
            .arg("--tsconfig")
            .arg(tsconfig)
            .arg(oracle)
            .arg(source_lock.worktree.join(source_module))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .expect("worktree-local tsx must start the TypeScript oracle");
        let stdin = BufWriter::new(child.stdin.take().expect("oracle stdin must be piped"));
        let stdout = BufReader::new(child.stdout.take().expect("oracle stdout must be piped"));
        Self { child, stdin, stdout }
    }

    pub(crate) fn request(&mut self, request: &Value) -> Value {
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

impl Drop for TsOracle {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    SourceLock {
        worktree: PathBuf::from(value["worktree"].as_str().expect("source lock worktree")),
        commit: value["commit"].as_str().expect("source lock commit").to_owned(),
    }
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

pub(crate) fn oracle_request(id: u64, op: &str, fields: Value) -> Value {
    let mut object = fields.as_object().cloned().unwrap_or_default();
    object.insert("id".to_owned(), json!(id));
    object.insert("op".to_owned(), json!(op));
    Value::Object(object)
}

pub(crate) fn expect_ok(response: &Value) -> &Value {
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    &response["output"]
}

pub(crate) fn compare(id: u64, op: &str, request: Value, rust: Value, oracle: &mut TsOracle) -> Value {
    let response = oracle.request(&request);
    assert_eq!(response["id"], json!(id));
    let output = expect_ok(&response);
    assert_eq!(output, &rust, "differential mismatch for {op} (request {request})");
    output.clone()
}

pub(crate) fn error_json(error: &SessionError) -> Value {
    json!({
        "name": "SessionError",
        "code": error.code().as_str(),
        "message": error.message(),
    })
}

fn normalize_path(path: &JsString, root: &str) -> JsString {
    let with_root = match path.strip_prefix(root) {
        Some(rest) => pi_js_compat::js_format!("<root>{}", rest),
        None => path.to_owned(),
    };
    JsString::join(
        with_root.split('/').map(|part| {
            if part.starts_with("--") && part.ends_with("--") {
                JsString::from("<sessionDir>")
            } else {
                part.to_owned()
            }
        }),
        "/",
    )
}

pub(crate) fn normalize_jsonl_metadata(metadata: &JsonlSessionMetadata, root: &str) -> Value {
    let mut object = serde_json::to_value(metadata).expect("metadata serializes");
    let map = object.as_object_mut().expect("metadata is an object");
    map.insert("cwd".to_owned(), json!(normalize_path(&metadata.cwd, root)));
    map.insert("path".to_owned(), json!(normalize_path(&metadata.path, root)));
    map.remove("modifiedAt");
    map.insert("modified".to_owned(), json!(metadata.modified_at > 0.0));
    object
}

pub(crate) fn normalize_session_metadata(metadata: &SessionMetadata, _root: &str) -> Value {
    serde_json::to_value(metadata).expect("metadata serializes")
}

pub(crate) fn entry_json(entry: &Entry) -> Value {
    serde_json::to_value(entry).expect("entry serializes")
}

pub(crate) fn entries_json(entries: &[Entry]) -> Value {
    Value::Array(entries.iter().map(entry_json).collect())
}

pub(crate) fn record_json(record: &LaneRecord) -> Value {
    serde_json::to_value(record).expect("record serializes")
}

pub(crate) fn records_json(records: &[LaneRecord]) -> Value {
    Value::Array(records.iter().map(record_json).collect())
}

fn entry_type(value: &Value) -> Option<EntryType> {
    value.as_str().and_then(|name| serde_json::from_value(json!(name)).ok())
}

fn record_type(value: &Value) -> Option<RecordType> {
    value.as_str().and_then(|name| serde_json::from_value(json!(name)).ok())
}

fn order(value: &Value) -> Option<EntryOrder> {
    match value.as_str()? {
        "oldestFirst" => Some(EntryOrder::OldestFirst),
        _ => Some(EntryOrder::NewestFirst),
    }
}

pub(crate) fn entry_query(value: &Value) -> EntryQuery {
    EntryQuery {
        entry_type: value.get("type").and_then(entry_type),
        custom_type: value.get("customType").and_then(Value::as_str).map(str::to_owned),
        order: value.get("order").and_then(order),
        limit: value.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
        cursor: value
            .get("cursor")
            .and_then(|cursor| cursor.get("afterSeq"))
            .and_then(Value::as_i64)
            .map(|after_seq| EntryCursor { after_seq }),
    }
}

pub(crate) fn branch_query(value: &Value) -> BranchQuery {
    BranchQuery {
        entry: entry_query(value),
        bounds: BranchBounds {
            start: value.get("start").and_then(Value::as_str).map(str::to_owned),
            stop_at_type: value.get("stopAtType").and_then(entry_type),
            stop_at_id: value.get("stopAtId").and_then(Value::as_str).map(str::to_owned),
        },
    }
}

pub(crate) fn record_query(value: &Value) -> RecordQuery {
    RecordQuery {
        lane: value.get("lane").and_then(Value::as_str).map(str::to_owned),
        record_type: value.get("type").and_then(record_type),
        run_id: value.get("runId").and_then(Value::as_str).map(str::to_owned),
        operation_kind: value
            .get("operationKind")
            .and_then(Value::as_str)
            .and_then(|kind| serde_json::from_value(json!(kind)).ok()),
        after_seq: value.get("afterSeq").and_then(Value::as_i64),
        order: value.get("order").and_then(order),
        limit: value.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
    }
}

pub(crate) fn read_files(root: &Path) -> pi_js_compat::js_json::JsObject {
    fn walk(directory: &Path, prefix: &str, root: &str, files: &mut pi_js_compat::js_json::JsObject) {
        for entry in std::fs::read_dir(directory).expect("session root must list") {
            let entry = entry.expect("directory entry");
            let name = entry.file_name().to_str().expect("utf-8 test path").to_owned();
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                walk(&path, &relative, root, files);
            } else {
                let content = std::fs::read_to_string(&path).expect("session file must read");
                let key = normalize_path(&pi_js_compat::js_format!("/{}", relative), root);
                let content = content.replace(root, "<root>");
                files.insert(key, pi_js_compat::js_json!(content));
            }
        }
    }
    let mut files = pi_js_compat::js_json::JsObject::new();
    walk(root, "", &root.to_str().expect("utf-8 test path"), &mut files);
    files
}

pub(crate) fn fixture_entry(value: Value) -> pi_agent_core::harness::session::EntryBody {
    serde_json::from_value(value).expect("entry fixture")
}

pub(crate) fn fixture_record(value: Value) -> pi_agent_core::harness::session::RecordBody {
    serde_json::from_value(value).expect("record fixture")
}
