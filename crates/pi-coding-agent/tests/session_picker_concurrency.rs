use pi_coding_agent::{
    cli::session_picker::{
        SelectSessionOptions, SelectSessionOutcome, SessionListFuture, SessionListProgress, SessionLoader,
        select_session,
    },
    core::{
        session_manager::SessionInfo,
        settings_manager::{SettingsManager, SettingsManagerCreateOptions},
    },
};
use pi_js_compat::js_json::{JsObject, JsString};
use pi_tui::terminal::{Terminal, TerminalEvent, TerminalRawModeError};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
struct State {
    started: bool,
    stopped: bool,
    keys: usize,
    writes: Vec<JsString>,
}
struct ScriptedTerminal(Arc<Mutex<State>>);
impl Terminal for ScriptedTerminal {
    fn start(&mut self) -> Result<(), TerminalRawModeError> {
        self.0.lock().unwrap().started = true;
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().stopped = true;
    }
    fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}
    fn write(&mut self, text: &JsString) {
        self.0.lock().unwrap().writes.push(text.clone());
    }
    fn columns(&self) -> usize {
        90
    }
    fn rows(&self) -> usize {
        24
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _lines: isize) {}
    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _title: &JsString) {}
    fn set_progress(&mut self, _active: bool) {}
    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        let mut state = self.0.lock().unwrap();
        let key = match state.keys {
            0 => "\t",
            1 => "\u{1b}",
            _ => return Vec::new(),
        };
        state.keys += 1;
        vec![TerminalEvent::Input(key.into())]
    }
}
struct PendingLoader(Arc<AtomicUsize>);
impl SessionLoader for PendingLoader {
    fn load<'a>(
        &'a self,
        progress: Option<SessionListProgress<'a>>,
    ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            if let Some(progress) = progress {
                progress(1, 9);
            }
            std::future::pending().await
        })
    }
}
#[test]
fn pending_current_and_all_loads_do_not_block_picker_escape() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "picker_probe", "--nocapture"])
        .env("PI_PICKER_PROBE", "1")
        .env("HOME", temp.path())
        .env("PI_CODING_AGENT_DIR", temp.path().join("agent"))
        .env("PI_PACKAGE_DIR", env!("CARGO_MANIFEST_DIR"))
        .current_dir(temp.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "picker blocked while current loader remains pending: {}",
                String::from_utf8(output.stderr).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("PICKER_CANCELLED_WITH_TWO_PENDING_LOADS")
    );
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "private bounded process probe"]
async fn picker_probe() {
    if std::env::var_os("PI_PICKER_PROBE").is_none() {
        return;
    }
    let settings = SettingsManager::in_memory(
        JsObject::new(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let state = Arc::new(Mutex::new(State::default()));
    let terminal = Rc::new(RefCell::new(ScriptedTerminal(state.clone())));
    let calls = Arc::new(AtomicUsize::new(0));
    let loader = Arc::new(PendingLoader(calls.clone()));
    let result = select_session(
        loader.clone(),
        loader,
        SelectSessionOptions {
            settings_manager: &settings,
            terminal,
            env: &BTreeMap::new(),
            control_db_path: None,
            archived_sessions_loader: None,
            validate_selection: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result, SelectSessionOutcome::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let state = state.lock().unwrap();
    assert!(state.started && state.stopped);
    assert_eq!(state.keys, 2);
    assert!(
        state.writes.iter().any(|text| text.contains("Loading 1/9")),
        "progress must render while both futures remain pending"
    );
    println!("PICKER_CANCELLED_WITH_TWO_PENDING_LOADS");
}
