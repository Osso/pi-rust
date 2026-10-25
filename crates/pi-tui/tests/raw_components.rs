use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::{
    r#box::BoxComponent,
    dynamic_border::DynamicBorder,
    loader::{Loader, LoaderIndicatorOptions},
    spacer::Spacer,
    text::StyleFn,
    truncated_text::TruncatedText,
};
use pi_tui::root_compositor::{RootCompositor, RootCompositorOptions, RootLayoutEntry};
use pi_tui::tui::{Component, component};
use serde::Serialize;
use serde_json::json;
use std::rc::Rc;

struct Lines(Vec<JsString>);
impl Component for Lines {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        self.0.clone()
    }
}

#[derive(Serialize)]
struct Output {
    box_lines: Vec<JsString>,
    box_cached: Vec<JsString>,
    truncated: Vec<JsString>,
    border: Vec<JsString>,
    spacer: Vec<JsString>,
    root: Vec<JsString>,
    loader: Vec<JsString>,
    loader_updated: Vec<JsString>,
}

#[test]
fn native_composition_and_simple_components_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0x61, 0xd800, 32, 0xdc00, 0x62],
    ] {
        for width in 0..=7 {
            for padding in 0..=1 {
                let response =
                    oracle.request(&json!({"operation":"components", "units":units, "width":width, "padding":padding}));
                assert_eq!(response["ok"], true, "{response}");
                let raw = JsString::from_utf16(units.clone());
                let style: StyleFn = Rc::new(|value| JsString::from("\x1b[44m") + value + "\x1b[0m");
                let mut boxed = BoxComponent::new(padding, padding, Some(style.clone()));
                boxed.add_child(component(Lines(vec![raw.clone()])));
                let mut truncated = TruncatedText::new(raw.clone(), padding, padding);
                let prefix = raw.clone();
                let mut border = DynamicBorder::new(Rc::new(move |value| prefix.clone() + value));
                let mut spacer = Spacer::new(2);
                let mut root = RootCompositor::new(RootCompositorOptions {
                    get_height: Rc::new(|| 5),
                    flow: vec![RootLayoutEntry::new(component(Lines(vec![raw.clone()])))],
                    bottom: vec![RootLayoutEntry::new(component(Lines(vec![raw.clone()])))],
                });
                let mut loader = Loader::new(
                    Rc::new(|| {}),
                    style.clone(),
                    style,
                    raw.clone(),
                    Some(LoaderIndicatorOptions {
                        frames: Some(vec![raw.clone()]),
                        interval_ms: None,
                    }),
                );
                let first_loader = loader.render(width);
                loader.set_message(raw.clone() + "x");
                let actual = Output {
                    box_lines: boxed.render(width),
                    box_cached: boxed.render(width),
                    truncated: truncated.render(width),
                    border: border.render(width),
                    spacer: spacer.render(width),
                    root: root.render(width),
                    loader: first_loader,
                    loader_updated: loader.render(width),
                };
                loader.stop();
                assert_eq!(
                    parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap(),
                    parse_lossless(response["json"].as_str().unwrap()).unwrap(),
                    "units={units:?} width={width} padding={padding}"
                );
            }
        }
    }
}
