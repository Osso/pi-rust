use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::{
        keybindings::KeybindingsManager,
        session_manager::{SessionEntry, SessionTreeNode},
    },
    modes::interactive::{
        components::tree_selector::{FilterMode, TreeSelectorComponent},
        theme::init_theme,
    },
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use pi_tui::{Component, Focusable};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
fn raw(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn tree(request: &Value) -> Vec<SessionTreeNode> {
    if let Some(roots) = request.get("roots") {
        return from_value(&JsValue::from(roots)).unwrap();
    }
    let nodes: Vec<_> = request["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut entry = JsValue::from(&v["entry"]);
            if let Some(units) = v.get("contentUnits") {
                *entry.get_mut("message").unwrap().get_mut("content").unwrap() = JsValue::String(raw(units));
            }
            if let Some(units) = v.get("textUnits") {
                entry
                    .get_mut("message")
                    .unwrap()
                    .get_mut("content")
                    .unwrap()
                    .as_array_mut()
                    .unwrap()[0]
                    .as_object_mut()
                    .unwrap()
                    .insert("text".into(), JsValue::String(raw(units)));
            }
            if let Some(values) = v["argumentUnits"].as_object() {
                for (key, units) in values {
                    entry
                        .get_mut("message")
                        .unwrap()
                        .get_mut("content")
                        .unwrap()
                        .as_array_mut()
                        .unwrap()[0]
                        .get_mut("arguments")
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .insert(key.as_str().into(), JsValue::String(raw(units)));
                }
            }
            let entry: SessionEntry = from_value(&entry).unwrap();
            SessionTreeNode {
                entry,
                children: vec![],
                label: v
                    .get("labelUnits")
                    .map(raw)
                    .or_else(|| v["label"].as_str().map(JsString::from)),
                label_timestamp: v["labelTimestamp"].as_str().map(JsString::from),
            }
        })
        .collect();
    let by_id: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.entry.id().cloned().unwrap(), i))
        .collect();
    let mut children = vec![Vec::new(); nodes.len()];
    let mut roots = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if let Some(parent) = node.entry.parent_id().and_then(|id| by_id.get(id)) {
            children[*parent].push(i);
        } else {
            roots.push(i);
        }
    }
    fn build(index: usize, nodes: &[SessionTreeNode], children: &[Vec<usize>]) -> SessionTreeNode {
        let mut node = nodes[index].clone();
        node.children = children[index].iter().map(|i| build(*i, nodes, children)).collect();
        node
    }
    roots.into_iter().map(|i| build(i, &nodes, &children)).collect()
}
fn snapshot(selector: &mut TreeSelectorComponent, events: &RefCell<Vec<Value>>, width: usize) -> Value {
    let lines: Vec<_> = selector.render(width).iter().map(|s| s.units().to_vec()).collect();
    let rows: Vec<_> = selector
        .tree_list()
        .borrow_mut()
        .render(width)
        .iter()
        .map(|s| s.units().to_vec())
        .collect();
    let children = selector.as_container().map(|c| {
        c.children()
            .iter()
            .flat_map(|child| child.borrow_mut().render(width))
            .map(|s| s.units().to_vec())
            .collect::<Vec<_>>()
    });
    let list = selector.tree_list().borrow();
    let node = list.get_selected_node();
    json!({"lines":lines,"rows":rows,"children":children,"selected":list.selected_entry_id(),"label":node.and_then(|n|n.label.as_ref()).map(|s|s.units().to_vec()),"search":list.get_search_query().units(),"events":events.borrow().clone()})
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
}
async fn native(request: &Value) -> Value {
    init_theme(Some(&"dark".into()), false).unwrap();
    let bindings = request["bindings"]
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, values)| {
                    (
                        key.as_str().into(),
                        values
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().into())
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    KeybindingsManager::new(bindings, None).install();
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let selected = events.clone();
    let cancel = events.clone();
    let labelled = events.clone();
    let filter = match request["filter"].as_str() {
        Some("all") => Some(FilterMode::All),
        Some("no-tools") => Some(FilterMode::NoTools),
        Some("user-only") => Some(FilterMode::UserOnly),
        Some("labeled-only") => Some(FilterMode::LabeledOnly),
        _ => None,
    };
    let mut selector = TreeSelectorComponent::new(
        tree(request),
        request["leaf"].as_str().map(JsString::from),
        request.get("height").map(number).unwrap_or(24.0),
        Rc::new(move |id| selected.borrow_mut().push(json!(["select", id.units()]))),
        Rc::new(move || cancel.borrow_mut().push(json!(["cancel"]))),
        Some(Rc::new(move |id, label| {
            labelled
                .borrow_mut()
                .push(json!(["label", id.units(), label.map(|s| s.units())]))
        })),
        request["selected"].as_str().map(JsString::from),
        filter,
    );
    let width = request["width"].as_u64().unwrap_or(90) as usize;
    let extra = pi_tui::tui::component(pi_tui::Text::new(
        JsString::from_utf16(vec![101, 120, 116, 114, 97, 0xd800]),
        0,
        0,
    ));
    let mut output = vec![snapshot(&mut selector, &events, width)];
    if request["drop"] == true {
        let list = Rc::downgrade(selector.tree_list());
        drop(selector);
        assert!(
            list.upgrade().is_none(),
            "dropped selector must release its callback cycle"
        );
        tokio::time::sleep(Duration::from_millis(130)).await;
        output.push(json!({"events":events.borrow().clone()}));
        return json!(output);
    }
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "key" => selector.handle_input(&raw(&step["units"])),
            "listKey" => selector.tree_list().borrow_mut().handle_input(&raw(&step["units"])),
            "focus" => selector.set_focused(step["value"] == true),
            "label" => selector.tree_list().borrow_mut().update_node_label(
                &step["id"].as_str().unwrap().into(),
                step["labelUnits"].as_array().map(|_| raw(&step["labelUnits"])).as_ref(),
                step["timestamp"].as_str().map(JsString::from).as_ref(),
            ),
            "invalidate" => selector.invalidate(),
            "add" => selector.add_child(extra.clone()),
            "remove" => selector.remove_child(&extra),
            "clear" => selector.clear(),
            "wait" => {
                tokio::time::sleep(Duration::from_millis(step["ms"].as_u64().unwrap())).await;
            }
            _ => panic!("unknown step {step}"),
        }
        output.push(snapshot(&mut selector, &events, width));
    }
    json!(output)
}
fn key(text: &str) -> Value {
    json!({"op":"key","units":text.encode_utf16().collect::<Vec<_>>()})
}
fn user(id: &str, parent: Option<&str>, content: &str) -> Value {
    json!({"entry":{"type":"message","id":id,"parentId":parent,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"user","content":content,"timestamp":0}}})
}
fn assistant(id: &str, parent: Option<&str>, content: Value) -> Value {
    json!({"entry":{"type":"message","id":id,"parentId":parent,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"assistant","content":content,"api":"anthropic-messages","provider":"fixture","model":"fixture","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":0}}})
}
async fn compare(cases: Vec<Value>) {
    // Native cases install process-global keybindings; keep each oracle pair isolated.
    static KEYBINDINGS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _keybindings_guard = KEYBINDINGS_LOCK.lock().await;
    let mut oracle = oracle::TsOracle::spawn("tree-selector-oracle.mts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let expected = oracle.request(case);
        let mut future = Box::pin(native(case));
        let result = std::future::poll_fn(|cx| {
            use std::{future::Future, task::Poll};
            match pi_coding_agent::utils::uncaught::catch_panics(|| future.as_mut().poll(cx)) {
                Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
                Ok(Poll::Pending) => Poll::Pending,
                Err(error) => Poll::Ready(Err(error)),
            }
        })
        .await;
        let actual = match result {
            Ok(output) => {
                assert_eq!(expected["ok"], true, "source case{index}: {expected}");
                output
            }
            Err(error) => {
                let error = pi_coding_agent::utils::uncaught::format_uncaught(error.as_ref());
                assert_eq!(json!({"ok":false,"error":error}), expected, "case{index}");
                continue;
            }
        };
        if actual != expected["output"] {
            let mut fields = std::collections::BTreeSet::new();
            for (step, (a, e)) in actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .enumerate()
            {
                for field in ["lines", "rows", "children", "selected", "label", "search", "events"] {
                    if a[field] != e[field] {
                        fields.insert(field);
                        eprintln!(
                            "mismatch case{index} step{step} {field}: native={} frozen={}",
                            a[field], e[field]
                        );
                    }
                }
            }
            eprintln!("case{index} fields{fields:?}");
            failures.push(index);
        }
    }
    assert!(failures.is_empty(), "differing tree cases {failures:?}");
}
#[tokio::test(flavor = "current_thread")]
async fn tree_container_label_focus_and_navigation_match_frozen() {
    let nodes = vec![
        user("u", None, "hello"),
        assistant("a", Some("u"), json!([{"type":"text","text":"answer"}])),
    ];
    let steps = vec![
        json!({"op":"focus","value":true}),
        key("L"),
        json!({"op":"focus","value":false}),
        json!({"op":"focus","value":true}),
        json!({"op":"key","units":[65,55296]}),
        key("\r"),
        json!({"op":"focus","value":false}),
        key("L"),
        key("\u{1b}"),
        json!({"op":"focus","value":true}),
        key("L"),
        key("\u{15}"),
        key("\r"),
        key("\u{1b}"),
        json!({"op":"invalidate"}),
    ];
    let mut cases = vec![
        json!({"nodes":[],"drop":true,"steps":[]}),
        json!({"nodes":nodes,"leaf":"a","steps":[{"op":"add"},{"op":"remove"},{"op":"add"},{"op":"clear"},key("L"),key("\u{1b}") ]}),
    ];
    for width in [0, 1, 2, 8, 30, 90] {
        cases.push(json!({"nodes":nodes,"leaf":"a","steps":steps,"width":width}));
    }
    cases.push(
        json!({"nodes":[],"steps":[key("\u{1b}[A"),key("\u{1b}[B"),key("\u{1b}[6~"),key("\u{1b}"),json!({"op":"wait","ms":130})]}),
    );
    tokio::task::LocalSet::new().run_until(compare(cases)).await;
}
#[tokio::test(flavor = "current_thread")]
async fn tree_terminal_height_numbers_match_frozen() {
    let nodes = vec![
        user("u", None, "hello"),
        assistant("a", Some("u"), json!([{"type":"text","text":"answer"}])),
    ];
    let cases=[json!(2),json!(13.5),json!("NaN"),json!("Infinity")].into_iter().map(|height|json!({"nodes":nodes,"leaf":"a","height":height,"steps":[key("\u{1b}[C"),key("\u{1b}[D"),key("\u{1b}[B"),key("\r")]})).collect();
    tokio::task::LocalSet::new().run_until(compare(cases)).await;
}

#[tokio::test(flavor = "current_thread")]
async fn tree_tool_arguments_and_entry_truthiness_match_frozen() {
    let mut cases = Vec::new();
    let values = [
        json!({"name":"read","args":{"path":42,"offset":1,"limit":0}}),
        json!({"name":"read","args":{"path":false,"file_path":["a","b"],"offset":null,"limit":null}}),
        json!({"name":"read","args":{"path":"p","offset":"2","limit":3}}),
        json!({"name":"grep","args":{"pattern":{"nested":1},"path":["x"]}}),
        json!({"name":"bash","args":{"command":[1,2]}}),
        json!({"name":"ls","args":{"path":[],"pattern":{"toString":null}}}),
        json!({"name":"read","args":{"path":{"toString":null}}}),
        json!({"name":"read","args":{"path":"p","offset":{"toString":false},"limit":2}}),
        json!({"name":"read","args":{"path":"p","offset":true,"limit":[2]}}),
        json!({"name":"read","args":{"path":"p","offset":[1,2],"limit":3}}),
        json!({"name":"find","args":{"pattern":false,"path":{"nested":1}}}),
        json!({"name":"unknown","args":{"toString":false,"2":2,"1":1}}),
        json!({"name":"write","id":"","args":{"path":"ignored-empty-id"}}),
        json!({"name":"read","args":{"path":"","offset":0,"limit":3},"argumentUnits":{"path":[112,55296],"offset":[50,57343]}}),
        json!({"name":"grep","args":{"path":"","pattern":""},"argumentUnits":{"path":[112,55296],"pattern":[57343]}}),
    ];
    for value in values {
        let call_id = value["id"].as_str().unwrap_or("call");
        let mut call = assistant(
            "a",
            None,
            json!([{"type":"toolCall","id":call_id,"name":value["name"],"arguments":value["args"]}]),
        );
        if let Some(units) = value.get("argumentUnits") {
            call["argumentUnits"] = units.clone();
        }
        let result = json!({"entry":{"type":"message","id":"r","parentId":"a","timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"toolResult","toolCallId":call_id,"toolName":value["name"],"content":[{"type":"text","text":"ok"}],"isError":false,"timestamp":0}}});
        cases.push(json!({"nodes":[call,result],"leaf":"r","filter":"all","steps":[]}));
    }
    let mut empty_error = assistant("a", None, json!([]));
    empty_error["entry"]["message"]["errorMessage"] = json!("");
    let title = json!({"entry":{"type":"session_info","id":"title","parentId":"a","timestamp":"2026-01-01T00:00:00.000Z","name":""}});
    cases.push(json!({"nodes":[empty_error,title],"leaf":"a","filter":"all","steps":[]}));
    cases.push(json!({"nodes":[{"entry":{"type":"compaction","id":"c","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","summary":"large","firstKeptEntryId":"none","tokensBefore":1e30}}],"leaf":"c","filter":"all","steps":[]}));
    tokio::task::LocalSet::new().run_until(compare(cases)).await;
}

fn metadata(kind: &str, id: &str, parent: Option<&str>, fields: Value) -> Value {
    let mut entry = json!({"type":kind,"id":id,"parentId":parent,"timestamp":"2026-01-01T00:00:00.000Z"});
    for (key, value) in fields.as_object().unwrap() {
        entry[key] = value.clone();
    }
    json!({"entry":entry})
}
#[tokio::test(flavor = "current_thread")]
async fn tree_filters_search_labels_and_entry_types_match_frozen() {
    let mut cases = Vec::new();
    let now = pi_js_compat::date::local_date_fields(pi_ai::request::now_millis()).unwrap();
    let month = if now.month == 0 { 7 } else { 1 };
    let timestamps = [
        pi_coding_agent::utils::iso_time::now_iso_string(),
        format!("{:04}-{month:02}-02T12:00:00.000Z", now.year),
        "2000-01-02T12:00:00.000Z".to_owned(),
    ];
    let dated: Vec<_> = timestamps
        .into_iter()
        .enumerate()
        .map(|(i, timestamp)| {
            let mut node = user(&format!("dated{i}"), None, "label date");
            node["label"] = json!("label");
            node["labelTimestamp"] = json!(timestamp);
            node
        })
        .collect();
    cases.push(json!({"nodes":dated,"leaf":"dated2","height":24,"steps":[key("T"),key("\u{1b}[B"),key("T")]}));
    let mut y = user("y", Some("same"), "branch Y");
    y["children"] = json!([]);
    let mut x = user("x", Some("same"), "branch X");
    x["children"] = json!([]);
    let mut root_b = user("same", None, "root B");
    root_b["children"] = json!([y]);
    let mut root_a = user("same", None, "root A");
    root_a["children"] = json!([x]);
    cases.push(json!({"roots":[root_b,root_a],"leaf":"x","steps":[]}));
    cases.push(json!({"nodes":[user("u",None,"custom bindings")],"leaf":"u","bindings":{"tui.select.up":["ctrl+alt+up"],"tui.select.down":["ctrl+alt+down"],"tui.editor.cursorLeft":["pageUp"],"tui.editor.cursorRight":["pageDown"],"app.tree.foldOrUp":[],"app.tree.unfoldOrDown":[]},"width":30,"steps":[]}));
    let long = format!("{}needle-after-preview", "a".repeat(220));
    cases.push(json!({"nodes":[user("long",None,&long)],"leaf":"long","steps":[key("needle-after-preview"),key("\r"),key("\u{1b}")]}));
    let custom_array = metadata(
        "custom_message",
        "array",
        None,
        json!({"customType":"fixture","content":[{"type":"text","text":long}],"display":true}),
    );
    let custom_text = metadata(
        "custom_message",
        "text",
        None,
        json!({"customType":"fixture","content":long,"display":true}),
    );
    cases.push(json!({"nodes":[custom_array,custom_text],"leaf":"text","filter":"all","steps":[key("needle-after-preview"),key("\r")]}));
    let mut empty_label = user("u", None, "empty label");
    empty_label["label"] = json!("");
    empty_label["labelTimestamp"] = json!("2026-01-01T00:00:00.000Z");
    cases.push(json!({"nodes":[empty_label],"leaf":"u","filter":"labeled-only","steps":[key("T"),json!({"op":"label","id":"u","labelUnits":[65],"timestamp":""}),json!({"op":"label","id":"u","labelUnits":[],"timestamp":"invalid"})]}));
    cases.push(json!({"nodes":[user("",None,"empty id")],"leaf":"","steps":[key("\u{1b}[1;5D"),key("no match"),key("\u{1b}"),key("\r")]}));
    let mut units = vec![97u16; 199];
    units.extend([0xd83d, 0xde00, 55296]);
    let mut raw_user = user("raw", None, "");
    raw_user["contentUnits"] = json!(units);
    cases.push(json!({"nodes":[raw_user],"leaf":"raw","width":240,"steps":[json!({"op":"key","units":[0xd83d]}),key("\u{7f}"),key("\r")]}));
    let mut nodes = vec![
        user("u", None, "hello"),
        assistant("a", Some("u"), json!([{"type":"text","text":"answer"}])),
    ];
    nodes.push(metadata("custom_message","custom",Some("a"),json!({"customType":"fixture","content":[{"type":"text","text":"first"},{"type":"image","mimeType":"image/png","data":"x"},{"type":"text","text":"second"}],"display":true})));
    nodes.push(metadata(
        "compaction",
        "compact",
        Some("custom"),
        json!({"summary":"summary","firstKeptEntryId":"u","tokensBefore":1500}),
    ));
    nodes.push(user("tail", Some("compact"), "after compaction"));
    nodes.push(metadata(
        "branch_summary",
        "branch",
        Some("tail"),
        json!({"summary":"branch","fromId":"a"}),
    ));
    nodes.push(metadata(
        "model_change",
        "model",
        Some("branch"),
        json!({"provider":"fixture","modelId":"model"}),
    ));
    nodes.push(metadata(
        "thinking_level_change",
        "thinking",
        Some("model"),
        json!({"thinkingLevel":"high"}),
    ));
    nodes.push(metadata(
        "custom",
        "entry",
        Some("thinking"),
        json!({"customType":"tag","data":{"n":1}}),
    ));
    nodes.push(metadata(
        "label",
        "label",
        Some("entry"),
        json!({"targetId":"u","label":null}),
    ));
    nodes.push(metadata(
        "session_info",
        "title",
        Some("label"),
        json!({"name":"title"}),
    ));
    nodes.push(metadata(
        "future_entry",
        "future",
        Some("title"),
        json!({"data":"ignored"}),
    ));
    nodes[0]["labelUnits"] = json!([76, 55296]);
    nodes[0]["labelTimestamp"] = json!("invalid date");
    for filter in ["all", "default", "user-only", "no-tools", "labeled-only"] {
        cases.push(json!({"nodes":nodes,"leaf":"future","filter":filter,"height":12,"steps":[key("T"),key("\u{1b}[A"),key("\u{1b}[C"),key("\u{1b}[D"),key("\u{1b}[1;5D"),key("\u{1b}[1;5C"),key("\u{f}"),key("\u{1b}[111;6u"),key("summary"),key("\u{1b}")]}));
    }
    let long_nodes: Vec<_> = (0..24)
        .map(|i| {
            user(
                &format!("n{i}"),
                if i > 0 { Some(format!("n{}", i - 1)) } else { None }.as_deref(),
                &format!("row {i}"),
            )
        })
        .collect();
    for height in [
        json!(-2),
        json!(13.5),
        json!("Infinity"),
        json!("-Infinity"),
        json!("NaN"),
    ] {
        cases.push(json!({"nodes":long_nodes,"leaf":"n23","height":height,"steps":[key("\u{1b}[C"),key("\u{1b}[D"),key("\r")]}));
    }
    for tokens in [-500.0, -1500.0, 4503599627370497000.0, 1e30] {
        cases.push(json!({"nodes":[metadata("compaction","c",None,json!({"summary":"round","firstKeptEntryId":"u","tokensBefore":tokens}))],"leaf":"c","steps":[]}));
    }
    tokio::task::LocalSet::new().run_until(compare(cases)).await;
}

#[path = "support/env_probe.rs"]
mod env_probe;
#[test]
fn empty_home_uses_userprofile_for_tree_tool_paths() {
    let temp = tempfile::tempdir().unwrap();
    env_probe::run_probe(
        "tree_path_env_probe",
        &[
            ("HOME", "".into()),
            ("USERPROFILE", "/profile".into()),
            ("COLORTERM", "truecolor".into()),
            ("PI_CODING_AGENT_DIR", temp.path().as_os_str().to_owned()),
        ],
    );
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "private environment probe"]
async fn tree_path_env_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let call = assistant(
        "a",
        None,
        json!([{"type":"toolCall","id":"call","name":"write","arguments":{"path":"/profile/file"}}]),
    );
    let result = json!({"entry":{"type":"message","id":"r","parentId":"a","timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"toolResult","toolCallId":"call","toolName":"write","content":[],"isError":false,"timestamp":0}}});
    tokio::task::LocalSet::new()
        .run_until(compare(vec![
            json!({"nodes":[call,result],"leaf":"r","filter":"all","steps":[]}),
        ]))
        .await;
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
