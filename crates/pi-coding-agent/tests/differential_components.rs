#![recursion_limit = "256"]

//! Differential test: coding-agent selector components render and handle input
//! exactly like the pinned TypeScript components.
//!
//! The TypeScript side runs through `oracle/coding-agent-components-oracle.ts`.
//! The Rust side runs in a probe child process so `PI_PACKAGE_DIR` /
//! `PI_CODING_AGENT_DIR` can point at fixture directories.

#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/oracle.rs"]
mod oracle;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use oracle::TsOracle;
use serde_json::{Value, json};
use tempfile::TempDir;

const SOURCE_DIRECTORY: &str = "packages/coding-agent/src/modes/interactive/components";
const PROBE_NAME: &str = "COMPONENT_PROBE_NAME";
const PROBE_OUT: &str = "COMPONENT_PROBE_OUT";

fn fixture_package_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("package fixture dir");
    fs::create_dir(directory.path().join("theme")).expect("theme dir");
    for name in ["dark.json", "light.json"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join(name),
            directory.path().join("theme").join(name),
        )
        .expect("copy built-in theme");
    }
    directory
}

fn fixture_agent_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("agent fixture dir");
    fs::create_dir(directory.path().join("themes")).expect("themes dir");
    directory
}

fn thinking_cases() -> Vec<Value> {
    vec![json!({
        "name": "thinking",
        "component": "thinking",
        "props": {
            "currentLevel": "medium",
            "availableLevels": ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"],
        },
        "width": 80,
        "steps": [
            {"key": "\u{1b}[B"},
            {"key": "\u{1b}[A"},
            {"key": "\u{1b}[A"},
            {"width": 40},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn theme_cases() -> Vec<Value> {
    vec![json!({
        "name": "theme",
        "component": "theme",
        "props": {"currentTheme": "dark"},
        "width": 80,
        "steps": [
            {"key": "\u{1b}[B"},
            {"key": "\u{1b}[A"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn show_images_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "show-images-yes",
            "component": "show_images",
            "props": {"currentValue": true},
            "width": 60,
            "steps": [{"key": "\u{1b}[B"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "show-images-no",
            "component": "show_images",
            "props": {"currentValue": false},
            "width": 60,
            "steps": [{"key": "\u{1b}[A"}, {"key": "\u{1b}"}],
        }),
    ]
}

fn extension_cases() -> Vec<Value> {
    vec![json!({
        "name": "extension",
        "component": "extension",
        "props": {"title": "Choose an action", "options": ["alpha", "beta", "gamma"]},
        "width": 70,
        "steps": [
            {"key": "j"},
            {"key": "j"},
            {"key": "j"},
            {"key": "k"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn approval_cases() -> Vec<Value> {
    vec![json!({
        "name": "approval",
        "component": "approval",
        "props": {"currentPreset": "llm-approved-deny"},
        "width": 100,
        "steps": [
            {"key": "j"},
            {"key": "\t"},
            {"key": "\r"},
            {"key": "\t"},
            {"key": "\r"},
            {"key": "\u{1b}"},
        ],
    })]
}

fn trust_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "trust-saved-parent",
            "component": "trust",
            "props": {
                "cwd": "/parent/project",
                "savedDecision": {"path": "/parent", "decision": true},
                "projectTrusted": true,
            },
            "width": 100,
            "steps": [{"key": "j"}, {"key": "k"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "trust-none",
            "component": "trust",
            "props": {"cwd": "/project", "savedDecision": null, "projectTrusted": false},
            "width": 100,
            "steps": [{"key": "j"}, {"key": "\r"}],
        }),
    ]
}

fn sandbox_cases() -> Vec<Value> {
    vec![
        json!({
            "name": "sandbox-global",
            "component": "sandbox",
            "props": {"currentProfile": "workspace-write"},
            "width": 100,
            "steps": [{"key": "j"}, {"key": "\t"}, {"key": "\t"}, {"key": "\r"}, {"key": "\u{1b}"}],
        }),
        json!({
            "name": "sandbox-session-override",
            "component": "sandbox",
            "props": {"currentProfile": "full-access", "sessionProfile": "read-only"},
            "width": 100,
            "steps": [{"key": "\t"}, {"key": "\t"}, {"key": "j"}, {"key": "\r"}],
        }),
    ]
}

fn user_message_cases() -> Vec<Value> {
    let messages: Vec<Value> = (1..=12)
        .map(|index| {
            json!({
                "id": format!("m{index}"),
                "text": format!("message {index}\nwith a second line that is quite long and should be truncated at narrow widths"),
            })
        })
        .collect();
    vec![
        json!({
            "name": "user-message-narrow",
            "component": "user_message",
            "props": {"messages": messages, "initialSelectedId": "m2"},
            "width": 30,
            "steps": [
                {"key": "\u{1b}[A"},
                {"key": "\u{1b}[A"},
                {"key": "\u{1b}[B"},
                {"key": "\r"},
                {"key": "\u{1b}"},
            ],
        }),
        json!({
            "name": "user-message-empty",
            "component": "user_message",
            "props": {"messages": [], "initialSelectedId": null},
            "width": 60,
            "steps": [{"key": "\u{1b}"}],
        }),
    ]
}

fn cases_for(component: &str) -> Vec<Value> {
    match component {
        "thinking" => thinking_cases(),
        "theme" => theme_cases(),
        "show_images" => show_images_cases(),
        "extension" => extension_cases(),
        "approval" => approval_cases(),
        "trust" => trust_cases(),
        "sandbox" => sandbox_cases(),
        "user_message" => user_message_cases(),
        other => panic!("unknown component {other}"),
    }
}

fn compare_component(component: &str) {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    let cases = cases_for(component);

    let mut oracle = TsOracle::spawn_with_environment(
        "coding-agent-components-oracle.ts",
        SOURCE_DIRECTORY,
        &[("PI_CODING_AGENT_DIR", agent.path().as_os_str().to_owned())],
    );
    let response = oracle.request(&json!({"id": 1, "operation": "render", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let output_path = agent.path().join(format!("{component}.json"));
    let environment: Vec<(&str, OsString)> = vec![
        ("PI_PACKAGE_DIR", package.path().as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR", agent.path().as_os_str().to_owned()),
        (PROBE_NAME, OsString::from(component)),
        (PROBE_OUT, output_path.as_os_str().to_owned()),
    ];
    env_probe::run_probe("components_probe_child", &environment);

    let content = fs::read_to_string(&output_path).expect("probe output must exist");
    let actual: Vec<Value> = serde_json::from_str(&content).expect("probe output must be JSON");
    assert_eq!(actual.len(), expected.len());
    for (index, (actual_case, expected_case)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_case, expected_case,
            "{component} case {index} ({}) diverged",
            expected_case["name"]
        );
    }
}

#[test]
fn thinking_selector_matches_typescript() {
    compare_component("thinking");
}

#[test]
fn theme_selector_matches_typescript() {
    compare_component("theme");
}

#[test]
fn show_images_selector_matches_typescript() {
    compare_component("show_images");
}

#[test]
fn extension_selector_matches_typescript() {
    compare_component("extension");
}

#[test]
fn approval_selector_matches_typescript() {
    compare_component("approval");
}

#[test]
fn trust_selector_matches_typescript() {
    compare_component("trust");
}

#[test]
fn sandbox_selector_matches_typescript() {
    compare_component("sandbox");
}

#[test]
fn user_message_selector_matches_typescript() {
    compare_component("user_message");
}

// ============================================================================
// Probe child: renders the Rust components for the same case list.
// ============================================================================

#[test]
#[ignore = "child process probe"]
fn components_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let component = std::env::var(PROBE_NAME).expect("COMPONENT_PROBE_NAME");
    let output_path = std::env::var(PROBE_OUT).expect("COMPONENT_PROBE_OUT");

    pi_tui::terminal_image::set_capabilities(pi_tui::terminal_image::TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    let theme_path = PathBuf::from(std::env::var("PI_PACKAGE_DIR").expect("PI_PACKAGE_DIR")).join("theme/dark.json");
    let theme = pi_coding_agent::modes::interactive::theme::load_theme_from_path(&theme_path, None)
        .expect("fixture theme must load");
    pi_coding_agent::modes::interactive::theme::set_theme_instance(std::sync::Arc::new(theme));

    let results: Vec<Value> = cases_for(&component)
        .into_iter()
        .map(|case| run_component_case(&case))
        .collect();
    fs::write(output_path, serde_json::to_string(&results).unwrap()).expect("write probe output");
}

fn run_component_case(case: &Value) -> Value {
    use std::cell::RefCell;
    use std::rc::Rc;

    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let width = case["width"].as_u64().unwrap() as usize;
    let steps = case["steps"].as_array().unwrap().clone();
    let component = case["component"].as_str().unwrap();
    let props = &case["props"];

    let mut renders: Vec<Vec<String>> = Vec::new();
    match component {
        "thinking" => {
            use pi_agent_core::ThinkingLevel;
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::thinking_selector::ThinkingSelectorComponent;

            let levels: Vec<ThinkingLevel> = props["availableLevels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|level| match level.as_str().unwrap() {
                    "off" => ThinkingLevel::Off,
                    "minimal" => ThinkingLevel::Minimal,
                    "low" => ThinkingLevel::Low,
                    "medium" => ThinkingLevel::Medium,
                    "high" => ThinkingLevel::High,
                    "xhigh" => ThinkingLevel::XHigh,
                    "max" => ThinkingLevel::Max,
                    _ => ThinkingLevel::Ultra,
                })
                .collect();
            let current = levels
                .iter()
                .copied()
                .find(|level| level.as_str() == props["currentLevel"].as_str().unwrap())
                .unwrap();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ThinkingSelectorComponent::new(
                current,
                &levels,
                Rc::new(move |level| event_sink.borrow_mut().push(format!("select:{}", level.as_str()))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "theme" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::theme_selector::ThemeSelectorComponent;

            let event_sink = Rc::clone(&events);
            let preview_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ThemeSelectorComponent::new(
                props["currentTheme"].as_str().unwrap(),
                Rc::new(move |name| event_sink.borrow_mut().push(format!("select:{name}"))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
                Rc::new(move |name| preview_sink.borrow_mut().push(format!("preview:{name}"))),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "show_images" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::show_images_selector::ShowImagesSelectorComponent;

            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ShowImagesSelectorComponent::new(
                props["currentValue"].as_bool().unwrap(),
                Rc::new(move |value| event_sink.borrow_mut().push(format!("select:{value}"))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
            );
            let list = selector.get_select_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        "extension" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::extension_selector::{
                ExtensionSelectorComponent, ExtensionSelectorOptions,
            };

            let options: Vec<String> = props["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option.as_str().unwrap().to_owned())
                .collect();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let toggle_sink = Rc::clone(&events);
            let mut selector = ExtensionSelectorComponent::new(
                props["title"].as_str().unwrap(),
                options,
                Rc::new(move |option| event_sink.borrow_mut().push(format!("select:{option}"))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
                ExtensionSelectorOptions {
                    on_toggle_tools_expanded: Some(Rc::new(move || {
                        toggle_sink.borrow_mut().push("toggle".to_owned());
                    })),
                    ..ExtensionSelectorOptions::default()
                },
            );
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "approval" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::permissions::presets::ApprovalPresetName;
            use pi_coding_agent::modes::interactive::components::approval_selector::{
                ApprovalSelectorComponent, ApprovalSelectorOptions,
            };

            let current = ApprovalPresetName::parse(props["currentPreset"].as_str().unwrap()).unwrap();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = ApprovalSelectorComponent::new(ApprovalSelectorOptions {
                current_preset: current,
                on_select: Rc::new(move |selection| {
                    event_sink.borrow_mut().push(format!(
                        "select:{}:{}",
                        selection.preset.as_str(),
                        selection.scope.as_str()
                    ));
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
            });
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "trust" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::trust_manager::{
                ProjectTrustDecision, ProjectTrustStoreEntry, ProjectTrustUpdate,
            };
            use pi_coding_agent::modes::interactive::components::trust_selector::{
                TrustSelectorComponent, TrustSelectorOptions,
            };

            let saved_decision = if props["savedDecision"].is_null() {
                None
            } else {
                Some(ProjectTrustStoreEntry {
                    path: props["savedDecision"]["path"].as_str().unwrap().to_owned(),
                    decision: if props["savedDecision"]["decision"].as_bool().unwrap() {
                        ProjectTrustDecision::Trusted
                    } else {
                        ProjectTrustDecision::Untrusted
                    },
                })
            };
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = TrustSelectorComponent::new(TrustSelectorOptions {
                cwd: props["cwd"].as_str().unwrap().to_owned(),
                saved_decision,
                project_trusted: props["projectTrusted"].as_bool().unwrap(),
                on_select: Rc::new(move |selection| {
                    let updates: Vec<Value> = selection
                        .updates
                        .iter()
                        .map(|update: &ProjectTrustUpdate| {
                            json!({
                                "path": update.path,
                                "decision": match update.decision {
                                    ProjectTrustDecision::Unknown => Value::Null,
                                    ProjectTrustDecision::Trusted => json!(true),
                                    ProjectTrustDecision::Untrusted => json!(false),
                                },
                            })
                        })
                        .collect();
                    event_sink
                        .borrow_mut()
                        .push(format!("select:{}:{}", selection.trusted, Value::Array(updates)));
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
            });
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "sandbox" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::core::permissions::presets::SandboxProfileName;
            use pi_coding_agent::modes::interactive::components::sandbox_selector::{
                SandboxSelectorComponent, SandboxSelectorOptions, SandboxSelectorSelection,
            };

            let parse_profile =
                |value: &Value| SandboxProfileName::parse(value.as_str().unwrap()).expect("known sandbox profile");
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = SandboxSelectorComponent::new(SandboxSelectorOptions {
                current_profile: parse_profile(&props["currentProfile"]),
                session_profile: props["sessionProfile"]
                    .as_str()
                    .map(|value| SandboxProfileName::parse(value).expect("known sandbox profile")),
                on_select: Rc::new(move |selection| {
                    let event = match selection {
                        SandboxSelectorSelection::Saved { profile, scope } => {
                            format!("select:{}:{}", profile.as_str(), scope.as_str())
                        }
                        SandboxSelectorSelection::Session { profile } => format!(
                            "select:{}:session",
                            profile.map(SandboxProfileName::as_str).unwrap_or("none")
                        ),
                    };
                    event_sink.borrow_mut().push(event);
                }),
                on_cancel: Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
            });
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, None, step, &mut current_width);
            }
        }
        "user_message" => {
            use pi_tui::tui::Component;

            use pi_coding_agent::modes::interactive::components::user_message_selector::{
                UserMessageItem, UserMessageSelectorComponent,
            };

            let messages: Vec<UserMessageItem> = props["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|message| UserMessageItem {
                    id: message["id"].as_str().unwrap().to_owned(),
                    text: message["text"].as_str().unwrap().to_owned(),
                })
                .collect();
            let event_sink = Rc::clone(&events);
            let cancel_sink = Rc::clone(&events);
            let mut selector = UserMessageSelectorComponent::new(
                messages,
                Rc::new(move |id| event_sink.borrow_mut().push(format!("select:{id}"))),
                Rc::new(move || cancel_sink.borrow_mut().push("cancel".to_owned())),
                props["initialSelectedId"].as_str(),
            );
            let list = selector.get_message_list();
            renders.push(selector.render(width));
            let mut current_width = width;
            for step in &steps {
                apply_step(&mut renders, &mut selector, Some(&list), step, &mut current_width);
            }
        }
        other => panic!("unknown component {other}"),
    }

    let events = events.borrow().clone();
    json!({"name": case["name"], "renders": renders, "events": events})
}

fn apply_step(
    renders: &mut Vec<Vec<String>>,
    component: &mut dyn pi_tui::tui::Component,
    input_target: Option<&pi_tui::tui::ComponentRef>,
    step: &Value,
    width: &mut usize,
) {
    if let Some(step_width) = step["width"].as_u64() {
        *width = step_width as usize;
    }
    if let Some(key) = step["key"].as_str() {
        match input_target {
            Some(target) => target.borrow_mut().handle_input(key),
            None => component.handle_input(key),
        }
    }
    renders.push(component.render(*width));
}
