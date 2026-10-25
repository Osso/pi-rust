use crate::shared_oracle as oracle;

use std::{cell::RefCell, rc::Rc};

use pi_coding_agent::{
    core::keybindings::KeybindingsManager,
    modes::interactive::{
        components::custom_editor::{CustomEditor, CustomEditorOptions},
        interactive_mode::AnyEditorHandle,
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::{
    Component, EditorComponent,
    components::editor::{EditorFixedCell, EditorHost, EditorTheme},
};
use serde_json::{Value, json};

struct Host;
impl EditorHost for Host {
    fn terminal_rows(&self) -> usize {
        24
    }
    fn request_render(&self) {}
    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        Rc::new(Self)
    }
}
impl EditorFixedCell for Host {
    fn place(&self, _: i64, _: i64) {}
    fn clear(&self) {}
    fn update(&self, _: &JsString) -> bool {
        false
    }
}

fn units(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit.as_u64().unwrap() as u16)
            .collect(),
    )
}

fn new_editor(bindings: &Value) -> CustomEditor {
    let config = bindings
        .as_object()
        .unwrap()
        .iter()
        .map(|(action, keys)| {
            (
                JsString::from(action.as_str()),
                keys.as_array()
                    .unwrap()
                    .iter()
                    .map(|key| JsString::from(key.as_str().unwrap()))
                    .collect(),
            )
        })
        .collect();
    CustomEditor::new(
        Rc::new(Host),
        EditorTheme::plain(),
        KeybindingsManager::new(config, None),
        Default::default(),
    )
    .unwrap()
}

fn request(bindings: Value, actions: Value, input: &[u16]) -> Value {
    json!({"bindings":bindings, "actions":actions, "text":[], "input":input, "history":[]})
}

fn direct_dispatch(request: &Value) -> Value {
    let mut editor = new_editor(&request["bindings"]);
    editor.set_text(&units(&request["text"]));
    editor.set_history(
        request["history"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| JsString::from(text.as_str().unwrap()))
            .collect(),
    );
    let events = Rc::new(RefCell::new(Vec::new()));
    for action in request["actions"].as_array().unwrap() {
        let name = action["name"].as_str().unwrap().to_owned();
        let handled = action["handled"].as_bool().unwrap();
        let output = events.clone();
        let initial_text = request["text"].clone();
        editor.on_action(
            name.clone(),
            Rc::new(move || {
                output.borrow_mut().push(json!([name, initial_text]));
                handled
            }),
        );
    }
    editor.handle_input(&units(&request["input"]));
    json!({"text":editor.get_text().units(),"history":editor.get_history().iter().map(|text| text.units().to_vec()).collect::<Vec<_>>(),"events":*events.borrow()})
}

#[test]
fn action_order_false_fallthrough_and_history_collision_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let mut history_collision = request(
        json!({"app.clear":["up"]}),
        json!([{"name":"app.clear","handled":true}]),
        &[27, 91, 65],
    );
    history_collision["history"] = json!(["old"]);
    let cases = [
        request(
            json!({"app.clear":["x"]}),
            json!([{"name":"app.clear","handled":false}]),
            &[120],
        ),
        request(
            json!({"app.clear":["x"],"app.suspend":["x"]}),
            json!([{"name":"app.clear","handled":false},{"name":"app.suspend","handled":true}]),
            &[120],
        ),
        request(
            json!({"app.clear":["x"],"app.suspend":["x"]}),
            json!([{"name":"app.clear","handled":false},{"name":"app.suspend","handled":false}]),
            &[120],
        ),
        request(
            json!({"app.clear":["x"]}),
            json!([{"name":"app.clear","handled":true},{"name":"app.clear","handled":false}]),
            &[120],
        ),
        request(
            json!({"app.clipboard.pasteImage":["x"],"app.clear":["x"]}),
            json!([{"name":"app.clear","handled":true}]),
            &[120],
        ),
        request(json!({}), json!([{"name":"app.interrupt","handled":false}]), &[27]),
        request(json!({}), json!([{"name":"app.exit","handled":false}]), &[4]),
        request(json!({}), json!([]), &[0xd800]),
        history_collision,
    ];
    let mut mismatches = Vec::new();
    for case in cases {
        let expected = oracle.request(&case);
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = direct_dispatch(&case);
        if actual != expected["output"] {
            mismatches.push(json!({"case":case,"actual":actual,"expected":expected["output"]}));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

fn mounted_dispatch(request: &Value) -> Value {
    let editor = Rc::new(RefCell::new(new_editor(&request["bindings"])));
    editor.borrow_mut().set_text(&units(&request["text"]));
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorder = |name: String, next: Option<JsString>| {
        let owner = Rc::downgrade(&editor);
        let events = events.clone();
        Rc::new(move || {
            let owner = owner.upgrade().unwrap();
            events
                .borrow_mut()
                .push(json!([name, owner.borrow().get_text().units()]));
            if let Some(next) = &next {
                owner.borrow_mut().set_text(next);
            }
        }) as Rc<dyn Fn()>
    };
    for action in request["actions"].as_array().unwrap() {
        let name = action["name"].as_str().unwrap();
        let record = recorder(name.to_owned(), action.get("text").map(units));
        let handled = action["handled"].as_bool().unwrap();
        editor.borrow_mut().on_action(
            name,
            Rc::new(move || {
                record();
                handled
            }),
        );
    }
    if let Some(shortcut) = request.get("shortcut") {
        let record = recorder("shortcut".into(), shortcut.get("text").map(units));
        let handled = shortcut["handled"].as_bool().unwrap();
        editor.borrow_mut().on_extension_shortcut = Some(Rc::new(move |_| {
            record();
            handled
        }));
    }
    if request["escape"] == true {
        editor.borrow_mut().on_escape = Some(recorder("escape".into(), None));
    }
    if request["exit"] == true {
        editor.borrow_mut().on_ctrl_d = Some(recorder("exit".into(), None));
    }
    if request["paste"] == true {
        editor.borrow_mut().on_paste_image = Some(recorder("paste".into(), None));
    }
    let mounted: Rc<RefCell<dyn EditorComponent>> = editor.clone();
    AnyEditorHandle(mounted).handle_input(&units(&request["input"]));
    json!({"text":editor.borrow().get_text().units(),"history":[],"events":*events.borrow()})
}

#[test]
fn mounted_callbacks_reenter_and_observe_prior_mutations_like_frozen() {
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let mut case = request(
        json!({"app.clear":["x"],"app.suspend":["x"]}),
        json!([
            {"name":"app.clear","handled":false,"text":[65]},
            {"name":"app.suspend","handled":false,"text":[0xd800]}
        ]),
        &[120],
    );
    case["shortcut"] = json!({"handled":false,"text":[66]});
    let mut shortcut_exit = request(json!({"app.exit":["x"]}), json!([]), &[120]);
    shortcut_exit["text"] = json!([65]);
    shortcut_exit["shortcut"] = json!({"handled":false,"text":[]});
    shortcut_exit["exit"] = json!(true);
    let mut escape = request(json!({}), json!([{"name":"app.interrupt","handled":false}]), &[27]);
    escape["escape"] = json!(true);
    let mut paste = request(json!({"app.clipboard.pasteImage":["x"]}), json!([]), &[120]);
    paste["paste"] = json!(true);
    let mut shortcut_wins = request(
        json!({"app.clipboard.pasteImage":["x"], "app.clear":["x"]}),
        json!([{"name":"app.clear","handled":true}]),
        &[120],
    );
    shortcut_wins["shortcut"] = json!({"handled":true,"text":[0xdc00]});
    shortcut_wins["paste"] = json!(true);
    for case in [case, shortcut_exit, escape, paste, shortcut_wins] {
        let expected = oracle.request(&case);
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(mounted_dispatch(&case), expected["output"], "{case}");
    }
}

#[test]
fn history_migration_filter_trim_cap_and_memory_only_match_frozen_database() {
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let oracle_directory = tempfile::tempdir().unwrap();
    let native_directory = tempfile::tempdir().unwrap();
    let mut legacy: Vec<_> = [Some("drop"), None, Some(" legacy ")]
        .into_iter()
        .map(|text| text.map(|text| text.encode_utf16().collect::<Vec<_>>()))
        .collect();
    legacy.insert(0, Some(vec![0xd800]));
    legacy.extend((0..105).map(|i| Some(format!("entry-{i}").encode_utf16().collect())));
    let additions = vec![
        vec![32, 110, 101, 119, 32],
        vec![110, 101, 119],
        vec![],
        vec![32],
        vec![0xdc00],
    ];
    let request =
        json!({"operation":"history", "directory":oracle_directory.path(), "legacy":legacy, "additions":additions});
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    let rows = legacy
        .iter()
        .map(|entry| match entry {
            Some(entry) => pi_js_compat::js_json::JsValue::String(JsString::from_utf16(entry.clone())),
            None => pi_js_compat::js_json::JsValue::Number(42.0),
        })
        .collect();
    let legacy_path = native_directory.path().join("legacy.json");
    let database = JsString::from(native_directory.path().join("control.db").display().to_string());
    std::fs::write(
        &legacy_path,
        pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::Array(rows)),
    )
    .unwrap();
    let mut editor = CustomEditor::new(
        Rc::new(Host),
        EditorTheme::plain(),
        KeybindingsManager::new(Default::default(), None),
        CustomEditorOptions {
            prompt_history_control_db_path: Some(database.clone()),
            legacy_prompt_history_path: Some(JsString::from(legacy_path.display().to_string())),
            prompt_history_filter: Some(Rc::new(|text| text != "drop")),
            ..Default::default()
        },
    )
    .unwrap();
    let snapshot = |editor: &CustomEditor| {
        editor
            .get_history()
            .iter()
            .map(|text| text.units().to_vec())
            .collect::<Vec<_>>()
    };
    let mut snapshots = vec![snapshot(&editor)];
    for text in additions {
        editor.add_to_history(&JsString::from_utf16(text)).unwrap();
        snapshots.push(snapshot(&editor));
    }
    editor.add_to_history_without_persistence(&"memory-only".into());
    snapshots.push(snapshot(&editor));
    let reopened = CustomEditor::new(
        Rc::new(Host),
        EditorTheme::plain(),
        KeybindingsManager::new(Default::default(), None),
        CustomEditorOptions {
            prompt_history_control_db_path: Some(database.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    snapshots.push(snapshot(&reopened));
    let connection = rusqlite::Connection::open(pi_js_compat::js_json::encode_node_utf8(&database)).unwrap();
    let mut statement = connection
        .prepare("SELECT hex(CAST(content AS BLOB)) FROM prompt_history ORDER BY id")
        .unwrap();
    let stored = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(json!({"snapshots":snapshots,"stored":stored}), expected["output"]);
}

#[test]
fn inherited_history_failure_keeps_memory_mutation_and_skips_duplicate_retry_like_frozen() {
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let oracle_directory = tempfile::tempdir().unwrap();
    let native_directory = tempfile::tempdir().unwrap();
    let expected = oracle.request(&json!({"operation":"historyFailure", "directory":oracle_directory.path()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let database = native_directory.path().join("control.db");
    let mut editor = CustomEditor::new(
        Rc::new(Host),
        EditorTheme::plain(),
        KeybindingsManager::new(Default::default(), None),
        CustomEditorOptions {
            prompt_history_control_db_path: Some(database.display().to_string().into()),
            ..Default::default()
        },
    )
    .unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TRIGGER deny_history BEFORE INSERT ON prompt_history BEGIN SELECT RAISE(ABORT, 'history-write-failed'); END;").unwrap();
    let inherited: &mut dyn EditorComponent = &mut editor;
    let outcomes: Vec<_> = ["first", "first", "second"]
        .into_iter()
        .map(|entry| match inherited.add_to_history(&entry.into()) {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"message":error.message}),
        })
        .collect();
    let stored: i64 = connection
        .query_row("SELECT count(*) FROM prompt_history", [], |row| row.get(0))
        .unwrap();
    let history: Vec<_> = editor.get_history().iter().map(|text| text.units().to_vec()).collect();
    assert_eq!(
        json!({"outcomes":outcomes,"history":history,"stored":stored}),
        expected["output"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn interrupt_cancels_autocomplete_before_dynamic_escape_like_frozen() {
    use pi_tui::autocomplete::{CombinedAutocompleteProvider, CommandEntry, SlashCommand};
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let expected = oracle.request(&json!({"operation":"autocomplete"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut editor = new_editor(&json!({}));
    editor.set_autocomplete_provider(Rc::new(CombinedAutocompleteProvider::new(
        vec![
            CommandEntry::Command(SlashCommand::new("alpha")),
            CommandEntry::Command(SlashCommand::new("beta")),
        ],
        "/tmp",
        None,
    )));
    editor.set_text(&"/".into());
    let events = Rc::new(RefCell::new(Vec::new()));
    let escaped = events.clone();
    editor.on_escape = Some(Rc::new(move || escaped.borrow_mut().push("escape")));
    let registered = events.clone();
    editor.on_action(
        "app.interrupt",
        Rc::new(move || {
            registered.borrow_mut().push("registered");
            false
        }),
    );
    editor.handle_input(&"\t".into());
    editor.editor_mut().poll_autocomplete().await;
    let before = editor.editor().is_showing_autocomplete();
    assert!(before, "fixture must open autocomplete before Escape");
    editor.handle_input(&"\x1b".into());
    let after = editor.editor().is_showing_autocomplete();
    editor.handle_input(&"\x1b".into());
    assert_eq!(
        json!({"before":before,"after":after,"events":*events.borrow(),"text":editor.get_text().units()}),
        expected["output"]
    );
}

#[test]
fn legacy_history_missing_read_parse_and_array_branches_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("custom-editor-oracle.ts", "packages/coding-agent/src");
    let cases: [(&str, Option<&[u8]>); 8] = [
        ("absent", None),
        ("missing", None),
        ("directory", None),
        ("malformed", Some(b"[")),
        ("object", Some(b"{}")),
        ("empty", Some(b"[]")),
        ("non-strings", Some(b"[null,42,true]")),
        ("invalid-utf8", Some(b"[\"\xff\"]")),
    ];
    for (kind, bytes) in cases {
        let oracle_directory = tempfile::tempdir().unwrap();
        let native_directory = tempfile::tempdir().unwrap();
        let expected = oracle.request(
            &json!({"operation":"legacyVariant", "kind":kind, "directory":oracle_directory.path(), "bytes":bytes}),
        );
        assert_eq!(expected["ok"], true, "{expected}");
        let legacy = native_directory.path().join("legacy.json");
        if let Some(bytes) = bytes {
            std::fs::write(&legacy, bytes).unwrap();
        }
        if kind == "directory" {
            std::fs::create_dir(&legacy).unwrap();
        }
        let editor = CustomEditor::new(
            Rc::new(Host),
            EditorTheme::plain(),
            KeybindingsManager::new(Default::default(), None),
            CustomEditorOptions {
                prompt_history_control_db_path: Some(
                    native_directory.path().join("control.db").display().to_string().into(),
                ),
                legacy_prompt_history_path: (kind != "absent").then(|| legacy.display().to_string().into()),
                ..Default::default()
            },
        )
        .unwrap();
        let actual: Vec<_> = editor.get_history().iter().map(|text| text.units().to_vec()).collect();
        assert_eq!(json!(actual), expected["output"], "{kind}");
    }
}

#[test]
fn empty_control_db_option_does_not_open_a_database() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("legacy.json");
    std::fs::write(&legacy, r#"["legacy"]"#).unwrap();
    let editor = CustomEditor::new(
        Rc::new(Host),
        EditorTheme::plain(),
        KeybindingsManager::new(Default::default(), None),
        CustomEditorOptions {
            prompt_history_control_db_path: Some(JsString::default()),
            legacy_prompt_history_path: Some(JsString::from(legacy.display().to_string())),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        editor.get_history().is_empty(),
        "empty path disables legacy migration in frozen constructor"
    );
}
