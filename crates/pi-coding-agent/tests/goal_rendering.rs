#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::goal::rendering::supervisor_status_entry_renderer;
use pi_coding_agent::core::extensions::types::EntryRenderOptions;
use pi_coding_agent::core::session_manager::CustomEntry;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme};
use pi_coding_agent::utils::ansi::strip_ansi;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_tui::tui::ComponentRef;
use std::cell::RefCell;
use std::rc::Rc;

fn theme() -> Theme {
    let colors = REQUIRED_COLORS
        .iter()
        .map(|key| (JsString::from(*key), ColorValue::Text("#808080".into())))
        .collect::<Vec<_>>();
    Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn goal_countdown_rejected_scoped_repaint_preserves_visible_text() {
    tokio::task::LocalSet::new().run_until(async {
        let renderer = supervisor_status_entry_renderer();
        let deadline = pi_js_compat::date::to_iso_string_value(pi_js_compat::date::js_now_millis() + 300.0).unwrap();
        let entry: CustomEntry = json::from_value(&json!({"type":"custom", "id":"timer", "parentId":null, "timestamp":"2026-10-25T00:00:00.000Z", "customType":"supervisor-status", "data":{"message":"waiting", "reviewAt":deadline}})).unwrap();
        let cleanups = Rc::new(RefCell::new(Vec::<Rc<dyn Fn()>>::new()));
        let collect = cleanups.clone();
        let options = EntryRenderOptions { expanded:false, session_id:"timer-session".into(), request_render:Rc::new(|| Ok(false)), register_cleanup:Rc::new(move |cleanup| collect.borrow_mut().push(cleanup)) };
        let component = renderer(&entry, &options, &theme()).unwrap();
        let before = component.borrow_mut().render(14);
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert_eq!(component.borrow_mut().render(14), before, "a root render must not update a rejected scoped countdown");
        for cleanup in cleanups.take() { cleanup(); }
    }).await;
}

#[tokio::test(flavor = "current_thread")]
async fn goal_countdown_owner_repaint_renders_due_without_shrinking_then_cleans_up() {
    use pi_coding_agent::bundled_extensions::goal::{
        rendering::create_supervisor_status_entry_renderer, wait_countdown::create_wait_countdown_refresher,
    };
    tokio::task::LocalSet::new().run_until(async {
        let directory = tempfile::tempdir().unwrap();
        let harness = bundled::Harness::new(Vec::new(), pi_coding_agent::core::extensions::create_extension_runtime(), bundled::HarnessOptions::new(directory.path().to_str().unwrap())).await;
        let ctx = harness.context();
        let refresher = create_wait_countdown_refresher();
        let renderer = create_supervisor_status_entry_renderer(refresher.clone());
        let deadline = pi_js_compat::date::to_iso_string_value(pi_js_compat::date::js_now_millis() + 250.0).unwrap();
        let entry: CustomEntry = json::from_value(&json!({"type":"custom", "id":"live-timer", "parentId":null, "timestamp":"2026-10-25T00:00:00.000Z", "customType":"supervisor-status", "data":{"message":"waiting", "reviewAt":deadline}})).unwrap();
        let target = Rc::new(RefCell::new(None::<ComponentRef>));
        let painted = Rc::new(RefCell::new(Vec::<Vec<JsString>>::new()));
        let callback_target = target.clone();
        let callback_painted = painted.clone();
        let cleanups = Rc::new(RefCell::new(Vec::<Rc<dyn Fn()>>::new()));
        let collect = cleanups.clone();
        let options = EntryRenderOptions { expanded:false, session_id:ctx.session_manager().unwrap().get_session_id().clone(), request_render:Rc::new(move || {
            let lines = callback_target.borrow().as_ref().unwrap().borrow_mut().render(14);
            callback_painted.borrow_mut().push(lines);
            Ok(true)
        }), register_cleanup:Rc::new(move |cleanup| collect.borrow_mut().push(cleanup)) };
        let component = renderer(&entry, &options, &theme()).unwrap();
        let before = component.borrow_mut().render(14);
        *target.borrow_mut() = Some(component.clone());
        refresher.start(ctx, &deadline.into()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let painted = painted.borrow();
        assert_eq!(painted.len(), 1, "expiration repaints once");
        assert_eq!(painted[0].len(), before.len(), "countdown reserves wrapped height");
        assert!(painted[0].iter().any(|line| strip_ansi(line).contains("Review due")));
        drop(painted);
        for cleanup in cleanups.take() { cleanup(); }
        refresher.clear_all();
    }).await;
}

#[test]
fn goal_rendering_grouped_entries_match_frozen_lifecycle() {
    let cases = json!([[
        {"id":"1", "session":"a", "displayId":"review", "message":"first"},
        {"id":"2", "session":"a", "displayId":"review", "message":"updated long message that wraps across lines"},
        {"id":"3", "session":"b", "displayId":"review", "message":"other session"},
        {"id":"4", "session":"a", "message":"ungrouped"},
        {"id":"5", "session":"a", "displayId":"review", "message":"final"},
        {"id":"1", "session":"a", "displayId":"review", "message":"ignored replay of old persisted entry"},
        {"id":"6", "session":"a", "displayId":"review", "message":"after cleanup", "cleanup":true}
    ]]);
    let mut oracle = oracle::TsOracle::spawn(
        "goal-rendering-oracle.ts",
        "packages/coding-agent/extensions/goal/src/rendering.ts",
    );
    let expected = oracle.request(&json!({"id":1, "cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let mut actual = Vec::new();
    for steps in cases.as_array().unwrap() {
        let renderer = supervisor_status_entry_renderer();
        let mut components = Vec::<ComponentRef>::new();
        let cleanups = Rc::new(RefCell::new(Vec::<Rc<dyn Fn()>>::new()));
        let mut snapshots = Vec::new();
        for step in steps.as_array().unwrap() {
            if step["cleanup"] == json!(true) {
                for cleanup in cleanups.take() {
                    cleanup();
                }
            }
            let entry: CustomEntry = json::from_value(&json!({"type":"custom", "id":step["id"], "parentId":null, "timestamp":"2026-10-25T00:00:00.000Z", "customType":"supervisor-status", "data":{"message":step["message"], "displayId":step["displayId"]}})).unwrap();
            let collect = cleanups.clone();
            let options = EntryRenderOptions {
                expanded: false,
                session_id: step["session"].as_string().unwrap().clone(),
                request_render: Rc::new(|| Ok(true)),
                register_cleanup: Rc::new(move |cleanup| collect.borrow_mut().push(cleanup)),
            };
            let rendered = renderer(&entry, &options, &theme());
            let returned = rendered.is_some();
            if let Some(component) = rendered {
                components.push(component);
            }
            let lines = components
                .iter()
                .map(|component| {
                    component
                        .borrow_mut()
                        .render(32)
                        .iter()
                        .map(strip_ansi)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            snapshots.push(json!({"returned":returned, "lines":lines}));
        }
        for cleanup in cleanups.take() {
            cleanup();
        }
        actual.push(json!(snapshots));
    }
    assert_eq!(json!(actual), expected["output"]);
}
