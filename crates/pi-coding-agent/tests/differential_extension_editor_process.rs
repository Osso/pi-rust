use crate::shared_oracle as oracle;

use std::{cell::RefCell, rc::Rc};

use pi_coding_agent::{
    core::keybindings::KeybindingsManager,
    modes::interactive::{
        components::extension_editor::{ExtensionEditorComponent, ExtensionEditorHost},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::{
    Component,
    components::editor::{EditorFixedCell, EditorHost},
};
use serde_json::{Value, json};

struct Host {
    events: Rc<RefCell<Vec<&'static str>>>,
    finished: Rc<tokio::sync::Notify>,
}
impl EditorHost for Host {
    fn terminal_rows(&self) -> usize {
        24
    }
    fn request_render(&self) {}
    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        Rc::new(Cell)
    }
}
struct Cell;
impl EditorFixedCell for Cell {
    fn place(&self, _: i64, _: i64) {}
    fn clear(&self) {}
    fn update(&self, _: &JsString) -> bool {
        false
    }
}
impl Host {
    fn record_event(&self, event: &'static str) {
        self.events.borrow_mut().push(event);
        flush_failure_trace(&self.events);
    }
}
fn flush_failure_trace(events: &RefCell<Vec<&'static str>>) {
    if let Some(path) = std::env::var_os("PI_EXTENSION_EDITOR_FAILURE_TRACE") {
        std::fs::write(path, serde_json::to_vec(&*events.borrow()).unwrap()).unwrap();
    }
}

fn failure_request() -> Option<Value> {
    match std::env::var("PI_EXTENSION_EDITOR_FAILURE_COMMAND") {
        Ok(request) => Some(serde_json::from_str(&request).unwrap()),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid fixture request: {error}"),
    }
}

impl ExtensionEditorHost for Host {
    fn suspend(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.record_event("stop");
        if failure_request().is_some_and(|request| request["stopError"] == true) {
            return Err(pi_js_compat::callback_error::CallbackError::new("stop fixture"));
        }
        Ok(())
    }
    fn resume(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.record_event("start");
        if failure_request().is_some_and(|request| request["resumeError"] == true) {
            return Err(pi_js_compat::callback_error::CallbackError::new(
                "Failed to enable terminal raw mode: resume fixture",
            ));
        }
        Ok(())
    }
    fn request_forced_render(&self) {
        self.record_event("render");
        self.finished.notify_one();
    }
}

fn install_theme() {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).into(), ColorValue::Index(0.0)))
        .collect();
    set_theme_instance(std::sync::Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessCase {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_command: Option<String>,
    report: std::path::PathBuf,
    explicit: bool,
    prefill: Vec<u16>,
    environment: std::collections::BTreeMap<String, Option<String>>,
    bindings: std::collections::BTreeMap<String, Vec<String>>,
    input: Vec<u16>,
    skip_wait: bool,
}

impl ProcessCase {
    fn new(command: &str, next: Option<&str>, report: &std::path::Path, explicit: bool) -> Self {
        Self {
            command: command.into(),
            next_command: next.map(str::to_owned),
            report: report.into(),
            explicit,
            prefill: vec![65, 0xd800, 0xd83d, 0xde00],
            environment: Default::default(),
            bindings: Default::default(),
            input: vec![7],
            skip_wait: false,
        }
    }
}

fn capture_editor_frame(editor: &mut ExtensionEditorComponent) -> Vec<Vec<u16>> {
    let colors = pi_js_compat::js_regex::JsRegex::with_flags("\\x1b\\[[0-9;]*m", "g").unwrap();
    editor
        .render(40)
        .into_iter()
        .map(|line| colors.replace_all(line, "").units().to_vec())
        .collect()
}

async fn native_case(request: &ProcessCase) -> Value {
    let events = Rc::new(RefCell::new(Vec::new()));
    let finished = Rc::new(tokio::sync::Notify::new());
    let submissions = Rc::new(RefCell::new(Vec::new()));
    let submitted = submissions.clone();
    pi_js_compat::process_env::set("VISUAL", &request.command);
    let bindings = request
        .bindings
        .iter()
        .map(|(name, keys)| {
            (
                name.as_str().into(),
                keys.iter().map(|key| key.as_str().into()).collect(),
            )
        })
        .collect();
    let cancelled = events.clone();
    let mut editor = ExtensionEditorComponent::new(
        Rc::new(Host {
            events: events.clone(),
            finished: finished.clone(),
        }),
        KeybindingsManager::new(bindings, None),
        &"Compose".into(),
        Some(&JsString::from_utf16(request.prefill.clone())),
        Rc::new(move |text| submitted.borrow_mut().push(text.units().to_vec())),
        Rc::new(move || cancelled.borrow_mut().push("cancel")),
        Default::default(),
        request.explicit.then(|| request.command.as_str().into()),
    );
    let focused = editor.as_focusable_mut().map(|editor| {
        editor.set_focused(true);
        editor.focused()
    });
    let before = capture_editor_frame(&mut editor);
    if let Some(next) = &request.next_command {
        pi_js_compat::process_env::set("VISUAL", next);
    }
    for (name, value) in &request.environment {
        match value {
            Some(value) => pi_js_compat::process_env::set(name, value),
            None => pi_js_compat::process_env::remove(name),
        }
    }
    editor.handle_input(&JsString::from_utf16(request.input.clone()));
    events.borrow_mut().push("input-returned");
    flush_failure_trace(&events);
    if !request.skip_wait {
        tokio::time::timeout(std::time::Duration::from_secs(3), finished.notified())
            .await
            .expect("external editor finished");
    }
    // A pre-await failure can finish synchronously, but its rejection runs later.
    tokio::task::yield_now().await;
    let after = capture_editor_frame(&mut editor);
    editor.handle_input(&"\r".into());
    let report = if request.report.exists() {
        let mut report: Value = serde_json::from_slice(&std::fs::read(&request.report).unwrap()).unwrap();
        let path = report.as_object_mut().unwrap().remove("path").unwrap();
        assert!(
            !std::path::Path::new(path.as_str().unwrap()).exists(),
            "temporary edit file must be removed"
        );
        report
    } else {
        Value::Null
    };
    json!({"events":*events.borrow(),"submissions":*submissions.borrow(),"report":report,"focused":focused,"before":before,"after":after})
}

#[tokio::test(flavor = "current_thread")]
async fn extension_editor_finally_restores_before_uncaught_read_failure_like_frozen_process() {
    const COMMAND: &str = "PI_EXTENSION_EDITOR_FAILURE_COMMAND";
    if let Some(request) = failure_request() {
        let command = request["command"].as_str().unwrap();
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        install_theme();
        tokio::task::LocalSet::new()
            .run_until(async {
                native_case(&ProcessCase::new(
                    command,
                    None,
                    std::path::Path::new("unused-after-failed-read"),
                    true,
                ))
                .await;
            })
            .await;
        panic!("read failure must reach uncaught process boundary");
    }
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("remove.py");
    std::fs::write(
        &script,
        "import pathlib, sys, time\ntime.sleep(0.08)\npathlib.Path(sys.argv[-1]).unlink()\n",
    )
    .unwrap();
    let command = format!("python3 {}", script.display());
    let cases = [
        ("read", json!({"command":command})),
        ("stop", json!({"command":command,"stopError":true})),
        ("resume", json!({"command":command,"resumeError":true})),
        (
            "write",
            json!({"command":command,"tmpdir":directory.path().join("missing")}),
        ),
        ("empty-file", json!({"command":"  "})),
        ("nul-file", json!({"command":"bad\0name"})),
        ("nul-arg", json!({"command":"python3 a\0b"})),
    ];
    let lock: Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let head = std::process::Command::new("git")
        .args(["-C", source.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        lock["commit"].as_str().unwrap()
    );
    let mut mismatches = Vec::new();
    for (name, request) in cases {
        let native_trace = directory.path().join(format!("native-{name}.json"));
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "extension_editor_finally_restores_before_uncaught_read_failure_like_frozen_process",
                "--nocapture",
            ])
            .env(COMMAND, request.to_string())
            .env("PI_EXTENSION_EDITOR_FAILURE_TRACE", &native_trace);
        if let Some(tmpdir) = request["tmpdir"].as_str() {
            child.env("TMPDIR", tmpdir);
        }
        let native = child.output().unwrap();
        let oracle_trace = directory.path().join(format!("oracle-{name}.json"));
        let frozen = std::process::Command::new(source.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(source.join("tsconfig.json"))
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../oracle/extension-editor-failure-oracle.ts"),
            )
            .arg(source.join("packages/coding-agent/src"))
            .arg(request.to_string())
            .arg(&oracle_trace)
            .output()
            .unwrap();
        assert_eq!(
            frozen.status.code(),
            Some(1),
            "{name}: {}",
            String::from_utf8_lossy(&frozen.stderr)
        );
        assert_eq!(
            native.status.code(),
            Some(1),
            "{name}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let expected: Value = serde_json::from_slice(&std::fs::read(oracle_trace).unwrap()).unwrap();
        let actual: Value = serde_json::from_slice(&std::fs::read(native_trace).unwrap()).unwrap();
        let error_line = |stderr: &[u8]| {
            let text = String::from_utf8_lossy(stderr);
            let line = text
                .lines()
                .find(|line| line.starts_with("Error:") || line.starts_with("TypeError"))
                .unwrap_or_else(|| panic!("missing uncaught error diagnostic: {text}"));
            pi_js_compat::js_regex::JsRegex::with_flags(r"pi-extension-editor-[0-9]+\.md", "g")
                .unwrap()
                .replace_all(line, "<temporary-file>")
        };
        let native_error = error_line(&native.stderr);
        let frozen_error = error_line(&frozen.stderr);
        if actual != expected || native_error != frozen_error {
            mismatches.push(json!({"case":name,"actual":actual,"expected":expected,"nativeError":native_error,"frozenError":frozen_error}));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

fn prepare_process_fixture(directory: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let script = directory.join("observe.py");
    let source = r#"#!/usr/bin/env python3
import json, os, pathlib, re, signal, sys, time
file = pathlib.Path(sys.argv[-1])
args = sys.argv[1:-1]
mode = args[0] if args else "default"
report = {"bytes": list(file.read_bytes()), "name": re.sub(r"-[0-9]+\.md$", "-<time>.md", file.name), "parent": str(file.parent), "args": args, "path": str(file)}
pathlib.Path(os.environ["PI_EXTENSION_EDITOR_REPORT"]).write_text(json.dumps(report))
time.sleep(0.08)
if mode == "signal": os.kill(os.getpid(), signal.SIGTERM)
if mode == "nonzero":
    file.write_bytes(b"must not apply")
    sys.exit(17)
file.write_bytes(b"A\xff\r\n\n" if mode == "invalid" else b"plain" if mode == "no-lf" else b"saved\n\n")
"#;
    std::fs::write(&script, source).unwrap();
    let bin = directory.join("bin");
    std::fs::create_dir(&bin).unwrap();
    if cfg!(target_os = "windows") {
        std::fs::write(
            bin.join("notepad.cmd"),
            format!("@python3 \"{}\" %*\r\n", script.display()),
        )
        .unwrap();
    } else {
        let nano = bin.join("nano");
        std::fs::write(&nano, source).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(nano, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    (script, bin)
}

fn process_cases(
    directory: &std::path::Path,
    script: &std::path::Path,
    bin: &std::path::Path,
) -> Vec<(String, ProcessCase)> {
    let command = |marker| format!("python3 {} {marker}  extra", script.display());
    let old = command("old");
    let new = command("new");
    let original_path = pi_js_compat::process_env::get("PATH").unwrap();
    let temporary = directory.join("custom-temp");
    std::fs::create_dir(&temporary).unwrap();
    let mut cases = Vec::new();
    for name in [
        "explicit",
        "visual",
        "nonzero",
        "signal",
        "invalid",
        "no-lf",
        "spawn-error",
        "editor",
        "default",
        "cancel",
        "tmpdir",
        "tmp",
        "temp",
        "temp-default",
    ] {
        let report = directory.join(format!("oracle-{name}.json"));
        let mut case = ProcessCase::new(&command(name), None, &report, true);
        case.environment.insert("EDITOR".into(), None);
        case.environment.insert("PATH".into(), Some(original_path.clone()));
        case.environment
            .insert("PI_EXTENSION_EDITOR_REPORT".into(), Some(report.display().to_string()));
        for key in ["TMPDIR", "TMP", "TEMP"] {
            case.environment.insert(key.into(), pi_js_compat::process_env::get(key));
        }
        match name {
            "explicit" | "visual" => {
                case.command = old.clone();
                case.next_command = Some(new.clone());
                case.explicit = name == "explicit";
            }
            "spawn-error" => case.command = format!("/missing-pi-editor-{}", std::process::id()),
            "editor" => {
                case.command.clear();
                case.next_command = Some(String::new());
                case.environment.insert("EDITOR".into(), Some(command("editor")));
            }
            "default" => {
                case.explicit = false;
                case.next_command = Some(String::new());
                let path = std::env::join_paths(
                    std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&original_path)),
                )
                .unwrap()
                .into_string()
                .unwrap();
                case.environment.insert("PATH".into(), Some(path));
            }
            "cancel" => {
                case.bindings
                    .insert("app.editor.external".into(), vec!["escape".into()]);
                case.input = vec![27];
                case.skip_wait = true;
            }
            "tmpdir" | "tmp" | "temp" | "temp-default" => {
                for key in ["TMPDIR", "TMP", "TEMP"] {
                    case.environment.insert(key.into(), None);
                }
                let key = match name {
                    "tmpdir" => Some("TMPDIR"),
                    "tmp" => Some("TMP"),
                    "temp" => Some("TEMP"),
                    _ => None,
                };
                if let Some(key) = key {
                    case.environment
                        .insert(key.into(), Some(temporary.display().to_string()));
                }
            }
            _ => {}
        }
        cases.push((name.into(), case));
    }
    cases
}

#[tokio::test(flavor = "current_thread")]
async fn extension_editor_real_process_order_focus_and_live_command_match_frozen() {
    tokio::task::LocalSet::new()
        .run_until(async {
            install_theme();
            let directory = tempfile::tempdir().unwrap();
            let (script, bin) = prepare_process_fixture(directory.path());
            let mut oracle = oracle::TsOracle::spawn("extension-editor-process-oracle.ts", "packages/coding-agent/src");
            let environment: Vec<_> = [
                "VISUAL",
                "EDITOR",
                "PATH",
                "PI_EXTENSION_EDITOR_REPORT",
                "TMPDIR",
                "TMP",
                "TEMP",
            ]
            .into_iter()
            .map(|name| (name, pi_js_compat::process_env::get(name)))
            .collect();
            let mut mismatches = Vec::new();
            for (name, request) in process_cases(directory.path(), &script, &bin) {
                let expected = oracle.request(&serde_json::to_value(&request).unwrap());
                assert_eq!(expected["ok"], true, "{name}: {expected}");
                let mut native = request;
                native.report = directory.path().join(format!("native-{name}.json"));
                native.environment.insert(
                    "PI_EXTENSION_EDITOR_REPORT".into(),
                    Some(native.report.display().to_string()),
                );
                let actual = native_case(&native).await;
                if actual != expected["output"] {
                    mismatches.push(json!({"case":name,"actual":actual,"expected":expected["output"]}));
                }
            }
            for (name, value) in environment {
                match value {
                    Some(value) => pi_js_compat::process_env::set(name, &value),
                    None => pi_js_compat::process_env::remove(name),
                }
            }
            assert!(mismatches.is_empty(), "{mismatches:#?}");
        })
        .await;
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
