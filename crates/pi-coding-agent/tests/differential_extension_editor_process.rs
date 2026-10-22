#[path = "support/oracle.rs"]
mod oracle;

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
        if let Some(path) = std::env::var_os("PI_EXTENSION_EDITOR_FAILURE_TRACE") {
            std::fs::write(path, serde_json::to_vec(&*self.events.borrow()).unwrap()).unwrap();
        }
    }
}
impl ExtensionEditorHost for Host {
    fn suspend(&self) {
        self.record_event("stop");
    }
    fn resume(&self) -> Result<(), pi_tui::terminal::TerminalRawModeError> {
        self.record_event("start");
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
        .map(|name| ((*name).into(), ColorValue::Index(0)))
        .collect();
    set_theme_instance(std::sync::Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
}

async fn native_case(command: &str, next: Option<&str>, report: &std::path::Path, explicit: bool) -> Value {
    let events = Rc::new(RefCell::new(Vec::new()));
    let finished = Rc::new(tokio::sync::Notify::new());
    let submissions = Rc::new(RefCell::new(Vec::new()));
    let submitted = submissions.clone();
    pi_js_compat::process_env::set("VISUAL", command);
    let mut editor = ExtensionEditorComponent::new(
        Rc::new(Host {
            events: events.clone(),
            finished: finished.clone(),
        }),
        KeybindingsManager::new(Default::default(), None),
        &"Compose".into(),
        Some(&JsString::from_utf16(vec![65, 0xd800, 0xd83d, 0xde00])),
        Rc::new(move |text| submitted.borrow_mut().push(text.units().to_vec())),
        Rc::new(|| {}),
        Default::default(),
        explicit.then(|| command.into()),
    );
    let focused = editor.as_focusable_mut().map(|editor| {
        editor.set_focused(true);
        editor.focused()
    });
    if let Some(next) = next {
        pi_js_compat::process_env::set("VISUAL", next);
    }
    editor.handle_input(&"\x07".into());
    events.borrow_mut().push("input-returned");
    tokio::time::timeout(std::time::Duration::from_secs(3), finished.notified())
        .await
        .expect("external editor finished");
    editor.handle_input(&"\r".into());
    let mut report: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    let path = report.as_object_mut().unwrap().remove("path").unwrap();
    assert!(
        !std::path::Path::new(path.as_str().unwrap()).exists(),
        "temporary edit file must be removed"
    );
    json!({"events":*events.borrow(),"submissions":*submissions.borrow(),"report":report,"focused":focused})
}

#[tokio::test(flavor = "current_thread")]
async fn extension_editor_finally_restores_before_uncaught_read_failure_like_frozen_process() {
    const COMMAND: &str = "PI_EXTENSION_EDITOR_FAILURE_COMMAND";
    if let Ok(command) = std::env::var(COMMAND) {
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        install_theme();
        tokio::task::LocalSet::new()
            .run_until(async {
                native_case(&command, None, std::path::Path::new("unused-after-failed-read"), true).await;
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
    let native_trace = directory.path().join("native.json");
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "extension_editor_finally_restores_before_uncaught_read_failure_like_frozen_process",
            "--nocapture",
        ])
        .env(COMMAND, &command)
        .env("PI_EXTENSION_EDITOR_FAILURE_TRACE", &native_trace)
        .output()
        .unwrap();
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
    let oracle_trace = directory.path().join("oracle.json");
    let frozen = std::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/extension-editor-failure-oracle.ts"))
        .arg(source.join("packages/coding-agent/src"))
        .arg(&command)
        .arg(&oracle_trace)
        .output()
        .unwrap();
    assert_eq!(
        frozen.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    assert_eq!(
        native.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let expected: Value = serde_json::from_slice(&std::fs::read(oracle_trace).unwrap()).unwrap();
    let actual: Value = serde_json::from_slice(&std::fs::read(native_trace).unwrap()).unwrap();
    assert_eq!(actual, expected, "finally must restart/render before uncaught error");
    assert!(String::from_utf8_lossy(&frozen.stderr).contains("ENOENT: no such file or directory, open"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("ENOENT: no such file or directory, open"));
}

#[tokio::test(flavor = "current_thread")]
async fn extension_editor_real_process_order_focus_and_live_command_match_frozen() {
    tokio::task::LocalSet::new().run_until(async {
        install_theme();
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("observe.py");
        std::fs::write(&script, r#"import json, pathlib, re, sys, time
file = pathlib.Path(sys.argv[-1])
report = {"bytes": list(file.read_bytes()), "name": re.sub(r"-[0-9]+\.md$", "-<time>.md", file.name), "args": sys.argv[2:-1], "path": str(file)}
pathlib.Path(sys.argv[1]).write_text(json.dumps(report))
time.sleep(0.08)
file.write_bytes(b"saved\n\n")
"#).unwrap();
        let mut oracle = oracle::TsOracle::spawn("extension-editor-process-oracle.ts", "packages/coding-agent/src");
        let previous_visual = pi_js_compat::process_env::get("VISUAL");
        let mut mismatches = Vec::new();
        for (index, explicit) in [true, false].into_iter().enumerate() {
            let oracle_report = directory.path().join(format!("oracle-{index}.json"));
            let native_report = directory.path().join(format!("native-{index}.json"));
            let command = |path: &std::path::Path, marker: &str| format!("python3 {} {} {marker}  extra", script.display(), path.display());
            let initial_oracle = command(&oracle_report,"old");
            let initial_native = command(&native_report,"old");
            let next_oracle = command(&oracle_report,"new");
            let next_native = command(&native_report,"new");
            let request = json!({"command":initial_oracle,"nextCommand":next_oracle,"report":oracle_report,"explicit":explicit,"prefill":[65,0xd800,0xd83d,0xde00]});
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"],true,"{expected}");
            let actual = native_case(&initial_native,Some(&next_native),&native_report,explicit).await;
            if actual != expected["output"] { mismatches.push(json!({"explicit":explicit,"actual":actual,"expected":expected["output"]})); }
        }
        match previous_visual { Some(value) => pi_js_compat::process_env::set("VISUAL",&value), None => pi_js_compat::process_env::remove("VISUAL") }
        assert!(mismatches.is_empty(),"{mismatches:#?}");
    }).await;
}
