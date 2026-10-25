use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::components::status_indicator::{StatusIndicator, StatusIndicatorKind};
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::tui::Component;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[test]
fn raw_status_message_callbacks_and_renders_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-coding-tui-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let expected = oracle.request(&json!({"operation":"status","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let requests = Rc::new(Cell::new(0));
        let sink = requests.clone();
        let callbacks = Rc::new(RefCell::new(Vec::new()));
        let sink_style = callbacks.clone();
        let mut status = StatusIndicator::new(
            StatusIndicatorKind::Working,
            Rc::new(move || sink.set(sink.get() + 1)),
            Rc::new(|text| JsString::from("<") + text + ">"),
            Rc::new(move |text| {
                sink_style.borrow_mut().push(text.clone());
                JsString::from("[") + text + "]"
            }),
            raw.clone(),
            None,
        );
        let mut snapshots = Vec::new();
        for width in [1, 4, 12] {
            snapshots.push(status.render(width));
            status.set_message(raw.clone() + "updated");
            snapshots.push(status.render(width));
        }
        status.dispose();
        let actual = serde_json::to_string(&(snapshots, callbacks.borrow().clone(), requests.get())).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}
