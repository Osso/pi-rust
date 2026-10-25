use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::{keybindings::KeybindingsManager, session_manager::SessionInfo},
    modes::interactive::{
        components::session_selector::{ProgressCallback, SessionListLoader, SessionSelectorComponent},
        theme::init_theme,
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::{Component, Focusable};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Instant};
struct Load {
    scope: String,
    progress: ProgressCallback,
    sender: Option<tokio::sync::oneshot::Sender<Result<Vec<SessionInfo>, JsString>>>,
}
fn loader(scope: &'static str, events: Rc<RefCell<Vec<Value>>>, loads: Rc<RefCell<Vec<Load>>>) -> SessionListLoader {
    Rc::new(move |progress| {
        events.borrow_mut().push(json!(["load", scope]));
        let (sender, receiver) = tokio::sync::oneshot::channel();
        loads.borrow_mut().push(Load {
            scope: scope.to_owned(),
            progress: progress.unwrap(),
            sender: Some(sender),
        });
        Box::pin(async move { receiver.await.unwrap() })
    })
}
fn session(scope: &str) -> SessionInfo {
    SessionInfo {
        path: format!("/sessions/{scope}.jsonl").into(),
        id: scope.into(),
        cwd: "/fixture".into(),
        name: Some(format!("name {scope}").into()),
        is_archived: None,
        parent_session_path: None,
        created: 7258118400000.0,
        modified: 7258118400000.0,
        message_count: 2,
        first_message: "fixture".into(),
    }
}
fn session_items(scope: &str, step: &Value) -> Vec<SessionInfo> {
    let Some(items) = step["items"].as_array() else {
        return vec![session(scope)];
    };
    items
        .iter()
        .map(|item| {
            let mut value = session(scope);
            if let Some(path) = item["path"].as_str() {
                value.path = path.into();
            }
            if let Some(cwd) = item["cwd"].as_str() {
                value.cwd = cwd.into();
            }
            value.parent_session_path = item["parent"].as_str().map(JsString::from);
            if let Some(name) = item.get("name") {
                value.name = name.as_str().map(JsString::from);
            }
            if let Some(units) = item["nameUnits"].as_array() {
                value.name = Some(JsString::from_utf16(
                    units.iter().map(|u| u.as_u64().unwrap() as u16).collect(),
                ));
            }
            if let Some(count) = item["count"].as_u64() {
                value.message_count = count as usize;
            }
            value
        })
        .collect()
}
fn snapshot(selector: &mut SessionSelectorComponent, events: &RefCell<Vec<Value>>, width: usize) -> Value {
    let lines: Vec<_> = selector.render(width).iter().map(|s| s.units().to_vec()).collect();
    let children = selector.as_container().map(|container| {
        container
            .children()
            .iter()
            .flat_map(|c| c.borrow_mut().render(width))
            .map(|s| s.units().to_vec())
            .collect::<Vec<_>>()
    });
    json!({"lines":lines,"children":children,"events":events.borrow().clone(),"selected":selector.session_list().borrow().get_selected_session_path()})
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
}
fn native(request: &Value) -> Value {
    init_theme(Some(&"dark".into()), false).unwrap();
    KeybindingsManager::new(Vec::new(), None).install();
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let loads = Rc::new(RefCell::new(Vec::<Load>::new()));
    let selected = events.clone();
    let cancel = events.clone();
    let exit = events.clone();
    let render = events.clone();
    let rename = events.clone();
    let reject_rename = request["rejectRename"] == true;
    let mut selector = SessionSelectorComponent::new(
        loader("current", events.clone(), loads.clone()),
        loader("all", events.clone(), loads.clone()),
        Rc::new(move |path| selected.borrow_mut().push(json!(["select", path]))),
        Rc::new(move || cancel.borrow_mut().push(json!(["cancel"]))),
        Rc::new(move || exit.borrow_mut().push(json!(["exit"]))),
        Rc::new(move || render.borrow_mut().push(json!(["render"]))),
        Some(Rc::new(move |path, name| {
            rename.borrow_mut().push(json!(["rename", path, name]));
            Box::pin(async move {
                if reject_rename {
                    Err("rename rejected".into())
                } else {
                    Ok(())
                }
            })
        })),
        None,
        None,
        None,
        (request["noArchived"] != true).then(|| loader("archived", events.clone(), loads.clone())),
        None,
    );
    let width = request["width"].as_u64().unwrap_or(90) as usize;
    let extra = pi_tui::tui::component(pi_tui::Text::new(
        JsString::from_utf16(vec![101, 120, 116, 114, 97, 0xd800]),
        0,
        0,
    ));
    let mut output = vec![snapshot(&mut selector, &events, width)];
    for step in request["steps"].as_array().unwrap() {
        let index = step["index"].as_u64().unwrap_or(0) as usize;
        match step["op"].as_str().unwrap() {
            "progress" => {
                if let Some(load) = loads.borrow().get(index) {
                    (load.progress)(number(&step["loaded"]), number(&step["total"]));
                }
            }
            "resolve" | "reject" => {
                if let Some(load) = loads.borrow_mut().get_mut(index) {
                    let result = if step["op"] == "reject" {
                        Err(step["message"].as_str().unwrap().into())
                    } else {
                        Ok(session_items(&load.scope, step))
                    };
                    let _ = load.sender.take().unwrap().send(result);
                }
            }
            "key" => selector.handle_input(&step["data"].as_str().unwrap().into()),
            "listKey" => selector.as_container().unwrap().children()[5]
                .borrow_mut()
                .handle_input(&step["data"].as_str().unwrap().into()),
            "listFocus" => {
                if let Some(list) = selector.as_container().unwrap().children()[5]
                    .borrow_mut()
                    .as_focusable_mut()
                {
                    list.set_focused(step["value"] == true);
                }
            }
            "error" => selector.show_error(&step["message"].as_str().unwrap().into()),
            "focus" => selector.set_focused(step["value"] == true),
            "add" => selector.add_child(extra.clone()),
            "remove" => selector.remove_child(&extra),
            "clear" => selector.clear(),
            "invalidate" => selector.invalidate(),
            "wait" => std::thread::sleep(std::time::Duration::from_millis(step["ms"].as_u64().unwrap())),
            "confirmRejected" => {
                use std::{
                    future::Future,
                    task::{Context, Poll, Waker},
                };
                let value = step["value"].as_str().unwrap().into();
                let mut future = Box::pin(selector.confirm_rename(&value));
                let Poll::Ready(result) = future.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
                    panic!("expected immediate rejected rename");
                };
                events.borrow_mut().push(json!(["rejected", result.unwrap_err()]));
            }
            _ => panic!("unknown step {step}"),
        }
        selector.poll(Instant::now());
        output.push(snapshot(&mut selector, &events, width));
    }
    json!(output)
}
#[test]
fn constructor_progress_scope_concurrency_and_rename_match_frozen() {
    let cases = [
        json!({"steps":[{"op":"key","data":"\u{1b}[6~"},{"op":"resolve","index":0},{"op":"key","data":"\r"},{"op":"key","data":"\u{1b}[5~"},{"op":"key","data":"\r"}]}),
        json!({"steps":[{"op":"add"},{"op":"resolve","index":0},{"op":"invalidate"},{"op":"remove"},{"op":"add"},{"op":"clear"},{"op":"key","data":"\u{12}"},{"op":"key","data":"\u{1b}"}]}),
        json!({"noArchived":true,"steps":[{"op":"resolve","index":0},{"op":"key","data":"\t"},{"op":"resolve","index":1},{"op":"key","data":"\t"},{"op":"key","data":"\t"}]}),
        json!({"rejectRename":true,"steps":[{"op":"resolve","index":0},{"op":"key","data":"\u{12}"},{"op":"confirmRejected","value":"  rejected name  "}]}),
        json!({"steps":[{"op":"resolve","index":0},{"op":"error","message":"first"},{"op":"error","message":"second"},{"op":"wait","ms":4100},{"op":"error","message":"third"},{"op":"key","data":"\r"},{"op":"wait","ms":4100}]}),
        json!({"steps":[{"op":"key","data":"\u{1b}[B"},{"op":"resolve","index":0},{"op":"key","data":"\r"},{"op":"key","data":"\u{1b}[A"},{"op":"key","data":"\r"}]}),
        json!({"steps":[{"op":"listFocus","value":true},{"op":"resolve","index":0},{"op":"listKey","data":"\t"},{"op":"resolve","index":1},{"op":"listKey","data":"\r"}]}),
        json!({"steps":[{"op":"resolve","index":0},{"op":"key","data":"\t"},{"op":"resolve","index":1},{"op":"key","data":"\u{12}"},{"op":"key","data":"\r"},{"op":"key","data":"\r"},{"op":"progress","index":2,"loaded":3,"total":9},{"op":"resolve","index":3,"items":[{"name":"newest"}]},{"op":"resolve","index":2,"items":[{"name":"stale cache"}]},{"op":"key","data":"\t"},{"op":"resolve","index":4},{"op":"key","data":"\t"},{"op":"key","data":"\t"}]}),
        json!({"steps":[{"op":"progress","loaded":1.5,"total":-2.5},{"op":"progress","loaded":"NaN","total":"Infinity"},{"op":"resolve","index":0}]}),
        json!({"steps":[{"op":"progress","loaded":1,"total":3},{"op":"key","data":"\t"},{"op":"progress","index":0,"loaded":2,"total":3},{"op":"resolve","index":0},{"op":"progress","index":1,"loaded":4,"total":9},{"op":"key","data":"\t"},{"op":"resolve","index":1},{"op":"key","data":"\t"},{"op":"resolve","index":2},{"op":"key","data":"\t"},{"op":"key","data":"\t"},{"op":"key","data":"\u{0012}"},{"op":"key","data":"\r"},{"op":"resolve","index":3},{"op":"key","data":"\u{001b}"}]}),
        json!({"steps":[{"op":"reject","message":"load failed"},{"op":"key","data":"\t"},{"op":"resolve","index":1},{"op":"error","message":"explicit error"},{"op":"key","data":"\r"},{"op":"key","data":"\u{001b}"}]}),
    ];
    let mut oracle = oracle::TsOracle::spawn("session-selector-oracle.mts", "packages/coding-agent/src");
    for (i, case) in cases.iter().enumerate() {
        let expected = oracle.request(case);
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = native(case);
        for (step, (actual, expected)) in actual
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["output"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(actual, expected, "case{i} step{step}");
        }
    }
}

#[test]
fn session_rows_tree_aliases_narrow_widths_and_search_match_frozen() {
    let mut items: Vec<Value> = (0..14)
        .map(
            |i| json!({"path":format!("/sessions/row{i}.jsonl"),"name":format!("row {i}"),"count":if i==13 {0}else{2}}),
        )
        .collect();
    items[1]["name"] = json!("");
    items[2]["nameUnits"] = json!([65, 0, 66, 127, 55296, 32, 55357, 56832]);
    items[5]["parent"] = json!("/sessions/row0.jsonl");
    items[6]["parent"] = json!("/sessions/row5.jsonl");
    let keys = [
        "\u{1b}[6~",
        "\u{1b}[5~",
        "\u{13}",
        "\u{13}",
        "\u{10}",
        "\u{4}",
        "x",
        "\u{1b}",
        "row",
        "\r",
        "\u{1b}",
    ];
    let mut cases = Vec::new();
    for width in [0, 1, 8, 30, 90] {
        let mut steps = vec![json!({"op":"resolve","items":items})];
        steps.extend(keys.iter().map(|data| json!({"op":"key","data":data})));
        cases.push(json!({"width":width,"steps":steps}));
    }
    cases.push(json!({"steps":[{"op":"resolve","items":[{"path":"/sessions/one.jsonl","name":"first"},{"path":"/sessions/one.jsonl","name":"last"}]},{"op":"key","data":"\u{13}"},{"op":"key","data":"\u{13}"}]}));
    let mut oracle = oracle::TsOracle::spawn("session-selector-oracle.mts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for (case_index, case) in cases.iter().enumerate() {
        let expected = oracle.request(case);
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = native(case);
        if actual != expected["output"] {
            let step = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .position(|(a, e)| a != e)
                .unwrap();
            eprintln!("case{case_index} width{} first differing step{step}", case["width"]);
            let a = &actual[step];
            let e = &expected["output"][step];
            for field in ["events", "selected", "lines", "children"] {
                if a[field] != e[field] {
                    eprintln!("field{field}");
                }
            }
            let line = a["lines"]
                .as_array()
                .unwrap()
                .iter()
                .zip(e["lines"].as_array().unwrap())
                .position(|(a, e)| a != e)
                .unwrap();
            let decode = |v: &Value| {
                JsString::from_utf16(
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|u| u.as_u64().unwrap() as u16)
                        .collect(),
                )
            };
            eprintln!(
                "line{line} native{:?} source{:?}",
                decode(&a["lines"][line]),
                decode(&e["lines"][line])
            );
            failures.push(case_index);
        }
    }
    assert!(failures.is_empty(), "differing session list cases {failures:?}");
}

#[path = "support/env_probe.rs"]
mod env_probe;
fn run_session_home_probe(empty_home: bool) {
    let temp = tempfile::tempdir().unwrap();
    let mut env = vec![
        ("USERPROFILE", temp.path().as_os_str().to_owned()),
        ("COLORTERM", "truecolor".into()),
        ("PI_CODING_AGENT_DIR", temp.path().as_os_str().to_owned()),
    ];
    if empty_home {
        env.push(("HOME", "".into()));
    }
    env_probe::run_probe("session_home_probe", &env);
}
#[test]
fn empty_home_session_paths_match_frozen() {
    run_session_home_probe(true);
}
#[test]
fn missing_home_session_paths_match_frozen() {
    run_session_home_probe(false);
}
#[test]
#[ignore = "private environment probe"]
fn session_home_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let path = format!("{}/file.jsonl", std::env::var("USERPROFILE").unwrap());
    let request = json!({"steps":[{"op":"resolve","items":[{"path":path}]},{"op":"key","data":"\u{10}"}]});
    let mut oracle = oracle::TsOracle::spawn("session-selector-oracle.mts", "packages/coding-agent/src");
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(native(&request), expected["output"]);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
