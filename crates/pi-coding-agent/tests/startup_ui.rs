//! `cli/startup-ui.ts` predicates and theme loading.

#[path = "support/env_probe.rs"]
mod env_probe;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::cli::startup_ui::{
    is_official_distribution, load_startup_themes, load_themes, should_run_first_time_setup, show_startup_input,
    show_startup_selector,
};
use pi_coding_agent::config;
use pi_coding_agent::core::package_manager::ResolvedResource;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_tui::terminal::{Terminal, TerminalEvent};
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

struct ScriptedTerminal {
    keys: Mutex<Vec<String>>,
    output: Vec<JsString>,
    active: bool,
}

impl ScriptedTerminal {
    fn new(keys: Vec<String>) -> Self {
        Self {
            keys: Mutex::new(keys),
            output: Vec::new(),
            active: false,
        }
    }
}

impl Terminal for ScriptedTerminal {
    fn start(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.active = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.active = false;
        Ok(())
    }

    fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}

    fn write(&mut self, data: &JsString) {
        self.output.push(data.clone());
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
    let dir = std::env::temp_dir().join(format!("pi-startup-ui-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn dark_theme_json() -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json")).unwrap())
        .unwrap()
}

fn write_theme(dir: &Path, file: &str, name: &str) -> String {
    let mut theme = dark_theme_json();
    theme["name"] = json!(name);
    let path = dir.join(file);
    fs::write(&path, serde_json::to_string(&theme).unwrap()).unwrap();
    path.display().to_string()
}

fn resource(path: &str, enabled: bool) -> ResolvedResource {
    ResolvedResource {
        path: JsString::from(path.to_owned()),
        enabled,
        metadata: pi_coding_agent::core::package_manager::PathMetadata {
            source: JsString::from("test"),
            scope: pi_coding_agent::core::package_manager::SourceScope::User,
            origin: pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel,
            base_dir: None,
        },
    }
}

#[test]
fn official_distribution_requires_all_three_identifiers() {
    assert!(is_official_distribution("@earendil-works/pi-coding-agent", "pi", ".pi"));
    assert!(!is_official_distribution("other", "pi", ".pi"));
    assert!(!is_official_distribution(
        "@earendil-works/pi-coding-agent",
        "fork",
        ".pi"
    ));
    assert!(!is_official_distribution(
        "@earendil-works/pi-coding-agent",
        "pi",
        ".fork"
    ));
}

#[test]
fn first_time_setup_needs_official_distribution_experimental_flag_default_agent_dir_and_missing_settings() {
    let dir = temp_dir("first-time");
    let settings = dir.join("settings.json");
    let agent_env = config::env_agent_dir();

    assert!(should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1")])
    ));

    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "0")])
    ));
    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1"), (agent_env.as_str(), "/tmp/agent")])
    ));

    fs::write(&settings, "{}").unwrap();
    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1")])
    ));
}

#[test]
fn load_themes_skips_disabled_and_broken_entries_and_dedupes_names() {
    let dir = temp_dir("themes");
    let first = write_theme(&dir, "first.json", "custom");
    let duplicate = write_theme(&dir, "duplicate.json", "custom");
    let second = write_theme(&dir, "second.json", "other");
    let broken = dir.join("broken.json");
    fs::write(&broken, "{ not json").unwrap();

    let themes = load_themes(&[
        resource(&first, true),
        resource(&duplicate, true),
        resource(&second, true),
        resource(&broken.display().to_string(), true),
        resource(&first, false),
        resource(&dir.join("missing.json").display().to_string(), true),
    ]);
    let names: Vec<&JsString> = themes.iter().filter_map(|theme| theme.name.as_ref()).collect();
    assert_eq!(names, vec!["custom", "other"]);
}

#[tokio::test]
async fn load_startup_themes_uses_global_settings_only() {
    let dir = temp_dir("startup-themes");
    let agent_dir = dir.join("agent");
    fs::create_dir_all(agent_dir.join("themes")).unwrap();
    write_theme(&agent_dir.join("themes"), "custom.json", "startup");

    let settings_manager = SettingsManager::in_memory(
        (json!({ "themes": [agent_dir.join("themes/custom.json").display().to_string()] })
            .as_object()
            .unwrap()
            .clone())
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let themes = load_startup_themes(&settings_manager).await.unwrap();
    let names: Vec<&JsString> = themes.iter().filter_map(|theme| theme.name.as_ref()).collect();
    assert!(names.iter().any(|name| *name == "startup"), "{names:?}");
}

fn in_memory_settings() -> SettingsManager {
    SettingsManager::in_memory(
        serde_json::Map::new()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
}

#[test]
fn session_picker_validation_retry_cancel_and_exit_reach_the_terminal_boundary() {
    let output = env_probe::run_probe(
        "session_picker_ui_probe_child",
        &[(
            "PI_PACKAGE_DIR",
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).into_os_string(),
        )],
    );
    assert!(String::from_utf8(output.stdout).unwrap().contains("PICKER_UI_PROOF:ok"));
}

#[test]
#[ignore = "child process probe"]
fn session_picker_ui_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::cli::session_picker::{
        SelectSessionOptions, SelectSessionOutcome, SessionListFuture, SessionListProgress, SessionLoader,
        select_session,
    };
    use pi_coding_agent::core::session_manager::SessionInfo;
    use std::sync::Arc;
    struct Loader;
    impl SessionLoader for Loader {
        fn load<'a>(
            &'a self,
            _: Option<SessionListProgress<'a>>,
        ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
            Box::pin(async {
                Ok(vec![SessionInfo {
                    path: "/sessions/selected.jsonl".into(),
                    id: "selected".into(),
                    cwd: "/work".into(),
                    name: Some("Selectable session".into()),
                    is_archived: Some(false),
                    parent_session_path: None,
                    created: 0.0,
                    modified: 0.0,
                    message_count: 1,
                    first_message: "message".into(),
                }])
            })
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (keys, expected) in [
        (
            vec!["", "", "\r", "", "\r", "\u{1b}"],
            SelectSessionOutcome::Selected("/sessions/selected.jsonl".into()),
        ),
        (vec!["\u{1b}"], SelectSessionOutcome::Cancelled),
        (vec!["\u{3}"], SelectSessionOutcome::Exited),
    ] {
        let settings = in_memory_settings();
        let terminal = Rc::new(RefCell::new(ScriptedTerminal::new(
            keys.into_iter().map(str::to_owned).collect(),
        )));
        let terminal_interface: Rc<RefCell<dyn Terminal>> = terminal.clone();
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&attempts);
        let validate = Arc::new(move |path: &JsString| {
            assert_eq!(path, "/sessions/selected.jsonl");
            if seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err(JsString::from("Selection blocked once"))
            } else {
                Ok(())
            }
        });
        let selected = runtime
            .block_on(select_session(
                Arc::new(Loader),
                Arc::new(Loader),
                SelectSessionOptions {
                    settings_manager: &settings,
                    terminal: terminal_interface,
                    env: &env(&[("COLORFGBG", "0;15")]),
                    control_db_path: None,
                    archived_sessions_loader: None,
                    validate_selection: Some(validate),
                },
            ))
            .unwrap();
        assert_eq!(selected, expected);
        assert!(!terminal.borrow().active);
        if matches!(selected, SelectSessionOutcome::Selected(_)) {
            let output = JsString::join(terminal.borrow().output.iter(), "");
            assert!(output.to_utf8().unwrap().contains("Selection blocked once"));
            assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        }
    }
    println!("PICKER_UI_PROOF:ok");
}

fn run_selector_probe(key: &str) -> String {
    let dir = temp_dir("selector-probe");
    let out = dir.join("outcome.json");
    let output = env_probe::run_probe(
        "startup_selector_probe_child",
        &[
            ("PI_STARTUP_SELECTOR_KEY", key.into()),
            ("PI_STARTUP_SELECTOR_OUT", out.as_os_str().to_owned()),
            (
                "PI_PACKAGE_DIR",
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).into_os_string(),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "startup selector probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).expect("selector outcome")).unwrap();
    value["selected"].as_str().unwrap_or("cancelled").to_owned()
}

#[test]
#[ignore = "child probe entry point"]
fn startup_selector_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let key = std::env::var("PI_STARTUP_SELECTOR_KEY").expect("probe key");
    let out = std::env::var("PI_STARTUP_SELECTOR_OUT").expect("probe out path");
    let settings_manager = in_memory_settings();
    let terminal = Rc::new(RefCell::new(ScriptedTerminal::new(vec![key])));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let selected = runtime
        .block_on(show_startup_selector(
            &settings_manager,
            terminal,
            &env(&[("COLORFGBG", "0;15")]),
            &JsString::from("Pick one"),
            vec![
                (JsString::from("Yes"), "yes".to_owned()),
                (JsString::from("No"), "no".to_owned()),
            ],
        ))
        .expect("selector runs");
    fs::write(out, serde_json::to_string(&json!({ "selected": selected })).unwrap()).unwrap();
}

#[test]
fn startup_selector_returns_the_selected_value() {
    assert_eq!(run_selector_probe("\r"), "yes");
}

#[test]
fn startup_selector_returns_none_when_cancelled() {
    assert_eq!(run_selector_probe("\u{1b}"), "cancelled");
}

#[test]
#[ignore = "child probe entry point"]
fn startup_input_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let keys: Vec<String> =
        serde_json::from_str(&std::env::var("PI_STARTUP_INPUT_KEYS").expect("probe keys")).expect("probe keys json");
    let out = std::env::var("PI_STARTUP_INPUT_OUT").expect("probe out path");
    let settings_manager = in_memory_settings();
    let terminal = Rc::new(RefCell::new(ScriptedTerminal::new(keys)));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let value = runtime
        .block_on(show_startup_input(
            &settings_manager,
            terminal,
            &env(&[("COLORFGBG", "0;15")]),
            &JsString::from("Your name"),
            Some(&JsString::from("name")),
        ))
        .expect("input runs");
    fs::write(out, serde_json::to_string(&json!({ "value": value })).unwrap()).unwrap();
}

fn run_input_probe(keys: &[&str]) -> Option<String> {
    let dir = temp_dir("input-probe");
    let out = dir.join("outcome.json");
    let output = env_probe::run_probe(
        "startup_input_probe_child",
        &[
            ("PI_STARTUP_INPUT_KEYS", json!(keys).to_string().into()),
            ("PI_STARTUP_INPUT_OUT", out.as_os_str().to_owned()),
            (
                "PI_PACKAGE_DIR",
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).into_os_string(),
            ),
        ],
    );
    assert!(
        output.status.success(),
        "startup input probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&out).expect("input outcome")).unwrap();
    value["value"].as_str().map(str::to_owned)
}

#[test]
fn startup_input_returns_the_typed_text() {
    assert_eq!(run_input_probe(&["hello", "\r"]).as_deref(), Some("hello"));
}

#[test]
fn startup_input_returns_none_when_cancelled() {
    assert_eq!(run_input_probe(&["\u{1b}"]), None);
}
