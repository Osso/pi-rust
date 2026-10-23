#[path = "support/oracle.rs"]
mod oracle;
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
fn snapshot(selector: &mut SessionSelectorComponent, events: &RefCell<Vec<Value>>) -> Value {
    let lines: Vec<_> = selector.render(90).iter().map(|s| s.units().to_vec()).collect();
    let children = selector.as_container().map(|container| {
        container
            .children()
            .iter()
            .flat_map(|c| c.borrow_mut().render(90))
            .map(|s| s.units().to_vec())
            .collect::<Vec<_>>()
    });
    json!({"lines":lines,"children":children,"events":events.borrow().clone(),"selected":selector.session_list().get_selected_session_path()})
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
    let mut selector = SessionSelectorComponent::new(
        loader("current", events.clone(), loads.clone()),
        loader("all", events.clone(), loads.clone()),
        Rc::new(move |path| selected.borrow_mut().push(json!(["select", path]))),
        Rc::new(move || cancel.borrow_mut().push(json!(["cancel"]))),
        Rc::new(move || exit.borrow_mut().push(json!(["exit"]))),
        Rc::new(move || render.borrow_mut().push(json!(["render"]))),
        Some(Rc::new(move |path, name| {
            rename.borrow_mut().push(json!(["rename", path, name]));
            Box::pin(async { Ok(()) })
        })),
        None,
        None,
        None,
        Some(loader("archived", events.clone(), loads.clone())),
        None,
    );
    let mut output = vec![snapshot(&mut selector, &events)];
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
                        Ok(vec![session(&load.scope)])
                    };
                    let _ = load.sender.take().unwrap().send(result);
                }
            }
            "key" => selector.handle_input(&step["data"].as_str().unwrap().into()),
            "error" => selector.show_error(&step["message"].as_str().unwrap().into()),
            "focus" => selector.set_focused(step["value"] == true),
            _ => panic!("unknown step {step}"),
        }
        selector.poll(Instant::now());
        output.push(snapshot(&mut selector, &events));
    }
    json!(output)
}
#[test]
fn constructor_progress_scope_concurrency_and_rename_match_frozen() {
    let cases = [
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
