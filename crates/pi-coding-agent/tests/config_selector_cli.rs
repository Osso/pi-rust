//! `cli/config-selector.ts` TUI wiring, driven through a fake terminal.
//!
//! `init_theme` loads built-in themes from the package directory, so the TUI
//! runs in a child process with `PI_PACKAGE_DIR` pointed at the crate (edition
//! 2024 forbids a safe `std::env::set_var` in-process).

#[path = "support/env_probe.rs"]
mod env_probe;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use pi_coding_agent::cli::config_selector::{ConfigSelectorOptions, SelectConfigOutcome, select_config};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_tui::terminal::{Terminal, TerminalEvent, TerminalRawModeError};
use serde_json::json;

const PROBE_KEY_ENV: &str = "PI_CONFIG_SELECTOR_KEY";
const PROBE_ADAPTER_ENV: &str = "PI_CONFIG_SELECTOR_ADAPTER";
const PROBE_OUT_ENV: &str = "PI_CONFIG_SELECTOR_OUT";

struct ScriptedTerminal {
    keys: Mutex<Vec<String>>,
    writes: Mutex<Vec<String>>,
}

impl ScriptedTerminal {
    fn new(keys: Vec<String>) -> Self {
        Self {
            keys: Mutex::new(keys),
            writes: Mutex::new(Vec::new()),
        }
    }
}

impl Terminal for ScriptedTerminal {
    fn start(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }

    fn stop(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }

    fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}

    fn write(&mut self, data: &JsString) {
        self.writes
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json::encode_node_utf8(data));
    }

    fn columns(&self) -> usize {
        80
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
        let mut keys = self.keys.lock().unwrap();
        if keys.is_empty() {
            return Vec::new();
        }
        vec![TerminalEvent::Input(JsString::from(keys.remove(0)))]
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-config-selector-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn in_memory_settings() -> Rc<RefCell<SettingsManager>> {
    Rc::new(RefCell::new(SettingsManager::in_memory(
        serde_json::Map::new()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )))
}

fn run_selector(key: &str, rebind_cancel: bool) -> SelectConfigOutcome {
    if rebind_cancel {
        pi_tui::set_keybindings(pi_tui::KeybindingsManager::new(
            pi_tui::tui_keybindings(),
            vec![(JsString::from("tui.select.cancel"), vec![JsString::from("escape")])],
        ));
    }
    let terminal = Rc::new(RefCell::new(ScriptedTerminal::new(vec![key.to_owned()])));
    let agent_dir = temp_dir("agent").display().to_string();
    select_config(
        ConfigSelectorOptions {
            resolved_paths: Default::default(),
            settings_manager: in_memory_settings(),
            cwd: JsString::from("/work"),
            agent_dir: JsString::from(agent_dir),
        },
        terminal,
    )
    .expect("selector starts")
}

fn run_probe(key: &str, rebind_cancel: bool) -> SelectConfigOutcome {
    let dir = temp_dir("probe");
    let out = dir.join("outcome.json");
    let mut env_pairs = vec![
        (PROBE_KEY_ENV, key.into()),
        (PROBE_OUT_ENV, out.as_os_str().to_owned()),
        (
            "PI_PACKAGE_DIR",
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).into_os_string(),
        ),
    ];
    if rebind_cancel {
        env_pairs.push(("PI_CONFIG_SELECTOR_REBIND", "1".into()));
    }
    let output = env_probe::run_probe("config_selector_probe_child", &env_pairs);
    assert!(
        output.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).expect("probe outcome")).unwrap();
    match value["outcome"].as_str().unwrap() {
        "closed" => SelectConfigOutcome::Closed,
        "exit" => SelectConfigOutcome::Exit,
        other => panic!("unexpected outcome {other}"),
    }
}

fn run_adapter_probe(key: &str) -> String {
    let dir = temp_dir("adapter-probe");
    let out = dir.join("outcome.json");
    let output = env_probe::run_probe(
        "config_selector_probe_child",
        &[
            (PROBE_KEY_ENV, key.into()),
            (PROBE_ADAPTER_ENV, "1".into()),
            (PROBE_OUT_ENV, out.as_os_str().to_owned()),
            (
                "PI_PACKAGE_DIR",
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).into_os_string(),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "adapter probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).expect("adapter outcome")).unwrap();
    value["outcome"].as_str().unwrap().to_owned()
}

#[test]
#[ignore = "child probe entry point"]
fn config_selector_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let key = std::env::var(PROBE_KEY_ENV).expect("probe key");
    let out = std::env::var(PROBE_OUT_ENV).expect("probe out path");
    if std::env::var(PROBE_ADAPTER_ENV).is_ok() {
        let dir = temp_dir("adapter");
        let terminal = Rc::new(RefCell::new(ScriptedTerminal::new(vec![key.clone()])));
        let selector = pi_coding_agent::cli::config_selector::TuiConfigSelector { terminal };
        let request = pi_coding_agent::package_manager_cli::SelectConfigRequest {
            resolved_paths: Default::default(),
            settings: std::sync::Arc::new(pi_coding_agent::core::package_manager::SettingsManagerStore::new(
                pi_coding_agent::core::settings_manager::SettingsManager::in_memory(
                    serde_json::Map::new()
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
                    SettingsManagerCreateOptions {
                        project_trusted: Some(false),
                    },
                ),
            )),
            cwd: JsString::from(dir.join("project").display().to_string()),
            agent_dir: JsString::from(dir.join("agent").display().to_string()),
        };
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(pi_coding_agent::package_manager_cli::ConfigSelector::select(
                &selector, request,
            ));
        let value = json!({ "outcome": if outcome.is_ok() { "adapter-ok" } else { "adapter-error" } });
        fs::write(out, serde_json::to_string(&value).unwrap()).unwrap();
        return;
    }
    let rebind_cancel = std::env::var("PI_CONFIG_SELECTOR_REBIND").is_ok();
    let outcome = run_selector(&key, rebind_cancel);
    let value = json!({
        "outcome": match outcome {
            SelectConfigOutcome::Closed => "closed",
            SelectConfigOutcome::Exit => "exit",
        }
    });
    fs::write(out, serde_json::to_string(&value).unwrap()).unwrap();
}

#[test]
fn escape_closes_the_selector() {
    assert_eq!(run_probe("\u{1b}", false), SelectConfigOutcome::Closed);
}

#[test]
fn ctrl_c_closes_through_the_default_cancel_binding() {
    // `tui.select.cancel` defaults to `["escape", "ctrl+c"]`, so ctrl+c takes
    // the onCancel branch before the explicit ctrl+c check.
    assert_eq!(run_probe("\u{3}", false), SelectConfigOutcome::Closed);
}

#[test]
fn ctrl_c_exits_when_the_cancel_binding_excludes_it() {
    assert_eq!(run_probe("\u{3}", true), SelectConfigOutcome::Exit);
}

#[test]
fn package_command_adapter_returns_ok_after_the_tui_closes() {
    assert_eq!(run_adapter_probe("\u{1b}"), "adapter-ok");
}
