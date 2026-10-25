use crate::shared_oracle as oracle;

use pi_ai::types::{Modality, Model, ModelCost};
use pi_coding_agent::{
    core::keybindings::KeybindingsManager,
    modes::interactive::{
        components::scoped_models_selector::{
            EnabledIds, ModelsCallbacks, ModelsConfig, ScopedModelsSelectorComponent,
        },
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
    utils::uncaught::{catch_panics, format_uncaught},
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn units(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn initialize(request: &Value) {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).into(), ColorValue::Index(0.0)))
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
    let bindings = request["bindings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(action, values)| {
            (
                action.as_str().into(),
                values
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap().into())
                    .collect(),
            )
        })
        .collect();
    KeybindingsManager::new(bindings, None).install();
}
fn model(value: &Value) -> Model {
    Model {
        properties: Default::default(),
        id: value["id"].as_str().unwrap().into(),
        name: value["name"].as_str().unwrap().into(),
        provider: value["provider"].as_str().unwrap().into(),
        api: "openai-completions".into(),
        base_url: JsString::default(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost::default(),
        context_window: 128000.0,
        auto_compaction_threshold: None,
        max_tokens: 4096.0,
        headers: None,
        compat: None,
    }
}
fn emit(events: &RefCell<Vec<Value>>, kind: &str, mut ids: EnabledIds, mutate: bool, fail: bool) {
    events.borrow_mut().push(
        json!({"kind":kind,"ids":ids.as_ref().map(|ids| ids.iter().map(|id| id.units().to_vec()).collect::<Vec<_>>())}),
    );
    if mutate && let Some(ids) = &mut ids {
        ids.push("callback/mutation".into());
    }
    if fail {
        pi_coding_agent::utils::uncaught::throw_uncaught("Error", pi_js_compat::js_format!("callback {}", kind));
    }
}
#[derive(Default)]
struct InvalidatingChild {
    invalidations: usize,
}
impl Component for InvalidatingChild {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        vec![pi_js_compat::js_format!(
            "extra{}{}",
            self.invalidations,
            JsString::from_utf16(vec![0xd800])
        )]
    }
    fn invalidate(&mut self) {
        self.invalidations += 1;
    }
}
fn native(request: &Value, uncaught: bool) -> Value {
    initialize(request);
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let change = events.clone();
    let persist = events.clone();
    let cancel = events.clone();
    let mutate = request["mutateCallbacks"].as_bool().unwrap_or(false);
    let fail_change = request["callbackError"] == "change";
    let fail_persist = request["callbackError"] == "persist";
    let fail_cancel = request["callbackError"] == "cancel";
    let mut selector = ScopedModelsSelectorComponent::new(
        ModelsConfig {
            all_models: request["models"].as_array().unwrap().iter().map(model).collect(),
            enabled_model_ids: request["enabled"].as_array().map(|ids| ids.iter().map(units).collect()),
        },
        ModelsCallbacks {
            on_change: Rc::new(move |ids| {
                if uncaught {
                    println!("change");
                }
                emit(&change, "change", ids, mutate, fail_change);
            }),
            on_persist: Rc::new(move |ids| {
                if uncaught {
                    println!("persist");
                }
                emit(&persist, "persist", ids, mutate, fail_persist);
            }),
            on_cancel: Rc::new(move || {
                cancel.borrow_mut().push(json!({"kind":"cancel"}));
                if fail_cancel {
                    pi_coding_agent::utils::uncaught::throw_uncaught("Error", "callback cancel");
                }
            }),
        },
    );
    let width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |selector: &mut ScopedModelsSelectorComponent, error: Option<JsString>| {
        let lines: Vec<_> = selector
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect();
        let search = selector.get_search_input();
        let (search_value, focused) = {
            let input = search.borrow();
            (input.get_value().units().to_vec(), pi_tui::Focusable::focused(&*input))
        };
        let mut snapshot =
            json!({"lines":lines,"search":search_value,"focused":focused,"events":events.borrow().clone()});
        if request["children"].as_bool().unwrap_or(false) {
            let children: Vec<_> = selector.as_container().map_or_else(Vec::new, |container| {
                container
                    .children()
                    .iter()
                    .flat_map(|child| child.borrow_mut().render(width))
                    .map(|line| line.units().to_vec())
                    .collect()
            });
            snapshot["children"] = json!(children);
        }
        if let Some(error) = error {
            snapshot["error"] = json!(error.to_utf8().unwrap());
        }
        snapshot
    };
    let mut snapshots = vec![snapshot(&mut selector, None)];
    let extra = pi_tui::tui::component(InvalidatingChild::default());
    for step in request["steps"].as_array().unwrap() {
        let mut apply = || match step["op"].as_str().unwrap() {
            "key" => selector.handle_input(&units(&step["units"])),
            "search" => {
                selector
                    .get_search_input()
                    .borrow_mut()
                    .set_value(units(&step["units"]));
            }
            "focus" => selector
                .as_focusable_mut()
                .unwrap()
                .set_focused(step["value"].as_bool().unwrap()),
            "invalidate" => selector.invalidate(),
            "add" => selector.add_child(extra.clone()),
            "remove" => selector.remove_child(&extra),
            "clear" => selector.clear(),
            value => panic!("unknown step {value}"),
        };
        let result = if uncaught {
            apply();
            Ok(())
        } else {
            catch_panics(apply)
        };
        snapshots.push(snapshot(
            &mut selector,
            result.err().map(|error| format_uncaught(error.as_ref())),
        ));
    }
    json!(snapshots)
}
fn key(data: &str) -> Value {
    json!({"op":"key","units":data.encode_utf16().collect::<Vec<_>>()})
}
fn search(data: &str) -> Value {
    json!({"op":"search","units":data.encode_utf16().collect::<Vec<_>>()})
}
fn fixture(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"provider":"provider"})
}
fn request(models: Vec<Value>, enabled: Option<Vec<&str>>, steps: Vec<Value>) -> Value {
    json!({"models":models,"enabled":enabled.map(|ids| ids.into_iter().map(|id| id.encode_utf16().collect::<Vec<_>>()).collect::<Vec<_>>()),"width":80,"steps":steps,"bindings":{},"mutateCallbacks":true})
}
#[test]
fn scoped_model_public_state_and_failure_sequences_match_frozen() {
    let base = vec![fixture("a", "Alpha"), fixture("b", "Beta")];
    let mut custom_cancel = request(base.clone(), None, vec![key("\u{1b}"), key("\u{11}")]);
    custom_cancel["bindings"] = json!({"tui.select.cancel":["ctrl+q"]});
    let mut inherited = request(
        base.clone(),
        None,
        vec![
            json!({"op":"focus","value":true}),
            json!({"op":"add"}),
            json!({"op":"invalidate"}),
            json!({"op":"remove"}),
            json!({"op":"focus","value":false}),
            json!({"op":"clear"}),
            key("\u{1b}[B"),
        ],
    );
    inherited["children"] = json!(true);
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let mut cases = vec![
        (
            "duplicate-model-last-write",
            request(
                vec![fixture("a", "first"), fixture("a", "last")],
                None,
                vec![key("\u{1b}[B")],
            ),
        ),
        (
            "live-search-input",
            request(
                base.clone(),
                None,
                vec![search("Beta"), key("\u{3}"), search("Beta"), key("\r")],
            ),
        ),
        ("custom-cancel-bindings", custom_cancel),
        ("inherited-container", inherited),
        (
            "filtered-reorder-underflow",
            request(
                base,
                Some(vec!["provider/a", "provider/b"]),
                vec![search("Beta"), key("\u{1b}[C"), key("\u{1b}[1;3A"), key("\u{1b}[B")],
            ),
        ),
    ];
    for enabled in [
        None,
        Some(vec![]),
        Some(vec!["provider/b"]),
        Some(vec!["missing/model", "provider/b"]),
        Some(vec!["provider/b", "provider/b"]),
    ] {
        let mut models = vec![fixture("a", "Alpha"), fixture("b", "Beta"), fixture("c", "Gamma")];
        models[2]["provider"] = json!("other");
        cases.push((
            "mutation-helpers",
            request(
                models,
                enabled,
                vec![
                    key("\u{1b}[A"),
                    key("\u{1b}[B"),
                    key("\u{1}"),
                    key("\u{18}"),
                    key("\r"),
                    key("\u{1b}[1;3A"),
                    key("\u{1b}[1;3B"),
                    key("\u{10}"),
                    key("\u{13}"),
                    key("Beta"),
                    key("\u{18}"),
                    key("\u{1}"),
                    key("\u{3}"),
                    key("\u{3}"),
                    key("\u{1b}"),
                ],
            ),
        ));
    }
    for kind in ["change", "persist", "cancel"] {
        let mut failure = request(
            vec![fixture("a", "Alpha"), fixture("b", "Beta")],
            None,
            vec![key("\r"), key("\u{13}"), key("\u{1b}")],
        );
        failure["callbackError"] = json!(kind);
        cases.push(("synchronous-callback-error", failure));
    }
    let mut raw = request(
        vec![fixture("a", "Alpha")],
        Some(vec![]),
        vec![
            json!({"op":"key","units":[55296]}),
            key("\r"),
            key("\u{3}"),
            key("\u{13}"),
        ],
    );
    raw["enabled"] = json!([[55296]]);
    cases.push(("raw-missing-id-and-search", raw));
    cases.push((
        "empty-model-list",
        request(
            vec![],
            None,
            vec![
                key("\u{1b}[A"),
                key("\u{1b}[B"),
                key("\r"),
                key("\u{10}"),
                key("\u{1b}[1;3A"),
                key("\u{1}"),
                key("\u{18}"),
                key("\u{13}"),
            ],
        ),
    ));
    let mut many = request(
        (0..12)
            .map(|id| fixture(&format!("m{id}"), &format!("Model {id}")))
            .collect(),
        None,
        (0..15).map(|_| key("\u{1b}[B")).collect(),
    );
    many["width"] = json!(24);
    cases.push(("scroll-and-wrap-narrow", many));
    let mut oracle = oracle::TsOracle::spawn("scoped-models-selector-oracle.ts", "packages/coding-agent/src");
    let mut mismatches = Vec::new();
    for (name, request) in cases {
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "source {name}: {expected}");
        let actual = native(&request, false);
        let expected = &expected["output"];
        if actual != *expected {
            let index = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected.as_array().unwrap())
                .position(|(actual, expected)| actual != expected)
                .unwrap();
            let fields: Vec<_> = ["lines", "children", "search", "events", "focused", "error"]
                .into_iter()
                .filter(|field| actual[index][field] != expected[index][field])
                .collect();
            eprintln!(
                "{name}: first differing step {index}; fields {fields:?}; source error {} / native error {}",
                expected[index]["error"], actual[index]["error"]
            );
            for field in ["lines", "children"] {
                if let (Some(actual_lines), Some(expected_lines)) =
                    (actual[index][field].as_array(), expected[index][field].as_array())
                {
                    for (line, (actual, expected)) in actual_lines.iter().zip(expected_lines).enumerate() {
                        if actual != expected {
                            eprintln!(
                                "first {field} line {line}: source {:?} / native {:?}",
                                units(expected),
                                units(actual)
                            );
                            break;
                        }
                    }
                }
            }
            mismatches.push(name);
        }
    }
    assert!(mismatches.is_empty(), "source/native mismatches: {mismatches:?}");
}

#[test]
fn filtered_reorder_uncaught_failure_matches_frozen_process_boundary() {
    use std::{path::PathBuf, process::Command};
    const CHILD: &str = "PI_SCOPED_UNCAUGHT_NATIVE";
    if std::env::var_os(CHILD).is_some() {
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        println!("before");
        native(
            &request(
                vec![fixture("a", "Alpha"), fixture("b", "Beta")],
                Some(vec!["provider/a", "provider/b"]),
                vec![search("Beta"), key("\u{1b}[C"), key("\u{1b}[1;3A")],
            ),
            true,
        );
        println!("after");
        return;
    }
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    let source = PathBuf::from(lock["worktree"].as_str().unwrap());
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        std::str::from_utf8(&head.stdout).unwrap().trim(),
        lock["commit"].as_str().unwrap()
    );
    let source_output = Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/scoped-models-selector-oracle.ts"))
        .arg(source.join("packages/coding-agent/src"))
        .env("PI_SCOPED_UNCAUGHT", "1")
        .output()
        .unwrap();
    let native_output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "filtered_reorder_uncaught_failure_matches_frozen_process_boundary",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    for (name, output) in [("source", &source_output), ("native", &native_output)] {
        assert_eq!(
            output.status.code(),
            Some(1),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let events: Vec<_> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .filter(|line| matches!(*line, "before" | "after" | "change" | "persist"))
            .collect();
        assert_eq!(events, ["before"], "{name} must fail before callbacks and continuation");
    }
    let source_stderr = std::str::from_utf8(&source_output.stderr).unwrap();
    let diagnostic = source_stderr
        .lines()
        .find(|line| line.starts_with("TypeError:"))
        .expect("frozen TypeError diagnostic");
    assert_eq!(
        diagnostic,
        "TypeError: Cannot read properties of undefined (reading 'model')"
    );
    assert_eq!(std::str::from_utf8(&native_output.stderr).unwrap().trim(), diagnostic);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
