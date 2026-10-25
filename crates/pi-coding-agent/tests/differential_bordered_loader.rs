use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::keybindings::KeybindingsManager,
    modes::interactive::{
        components::bordered_loader::{BorderedLoader, BorderedLoaderOptions},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, ThemeColor, set_theme_instance},
    },
};
use pi_js_compat::{
    abort::{AbortReason, AbortSignal},
    js_json::{JsString, JsValue},
};
use pi_tui::{
    Component,
    components::{cancellable_loader::CancellableLoader, loader::LoaderIndicatorOptions},
};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
};

fn colors(offset: usize) -> Value {
    let mut values = serde_json::Map::new();
    for (index, name) in REQUIRED_COLORS.iter().enumerate() {
        values.insert(
            (*name).into(),
            json!(format!(
                "#{:02x}{:02x}{:02x}",
                (16 + 7 * index + offset) % 256,
                (32 + 11 * index + offset) % 256,
                (48 + 13 * index + offset) % 256
            )),
        );
    }
    Value::Object(values)
}
fn theme(value: &Value) -> Arc<Theme> {
    let colors: Vec<_> = value
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| (name.as_str().into(), ColorValue::Text(value.as_str().unwrap().into())))
        .collect();
    Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap())
}
fn raw(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn reason(signal: &AbortSignal) -> Value {
    signal.reason().as_ref().map(reason_data).unwrap_or(Value::Null)
}
fn reason_data(reason: &AbortReason) -> Value {
    let error = reason.as_error().unwrap();
    let value = error.value();
    json!({"dom":matches!(reason,AbortReason::DomException(_)),"name":error.name,"message":error.message,"code":value.get("code").and_then(JsValue::as_f64).map(|code|code as i64)})
}
enum View {
    Bare(Box<CancellableLoader>),
    Bordered(BorderedLoader),
}
impl View {
    fn component(&mut self) -> &mut dyn Component {
        match self {
            Self::Bare(value) => value.as_mut(),
            Self::Bordered(value) => value,
        }
    }
    fn signal(&self) -> AbortSignal {
        match self {
            Self::Bare(value) => value.signal(),
            Self::Bordered(value) => value.signal(),
        }
    }
    fn handler(&mut self, handler: Option<Rc<dyn Fn()>>) {
        match self {
            Self::Bare(value) => value.on_abort = handler,
            Self::Bordered(value) => value.set_on_abort(handler),
        }
    }
}
fn native(request: &Value) -> Value {
    KeybindingsManager::new(Vec::new(), None).install();
    let captured = theme(&request["colors"]);
    let other = theme(&request["otherColors"]);
    set_theme_instance(if request["separateTheme"] == true {
        other.clone()
    } else {
        captured.clone()
    });
    let renders = Rc::new(Cell::new(0));
    let rendered = renders.clone();
    let request_render: Rc<dyn Fn()> = Rc::new(move || rendered.set(rendered.get() + 1));
    let mut view = if request["bare"] == true {
        let spinner = captured.clone();
        let message = captured.clone();
        View::Bare(Box::new(CancellableLoader::new(
            request_render,
            Rc::new(move |text| spinner.fg(ThemeColor::Accent, text)),
            Rc::new(move |text| message.fg(ThemeColor::Muted, text)),
            raw(&request["message"]),
            Some(LoaderIndicatorOptions {
                frames: Some(vec![]),
                interval_ms: None,
            }),
        )))
    } else {
        View::Bordered(BorderedLoader::new(
            request_render,
            captured,
            raw(&request["message"]),
            BorderedLoaderOptions {
                cancellable: request["cancellable"].as_bool(),
            },
        ))
    };
    let signal = view.signal();
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let observed = events.clone();
    signal.add_listener(move |reason| observed.lock().unwrap().push(json!(["abort", reason_data(reason)])));
    let handler = |tag: &str| {
        let observed = events.clone();
        let signal = signal.clone();
        let tag = tag.to_owned();
        Rc::new(move || {
            observed
                .lock()
                .unwrap()
                .push(json!(["callback", tag, signal.aborted(), reason(&signal)]))
        }) as Rc<dyn Fn()>
    };
    view.handler(Some(handler("first")));
    let snapshot = |view: &mut View| {
        let same = signal == view.signal();
        let component = view.component();
        let lines = component
            .render(50)
            .iter()
            .map(|line| line.units().to_vec())
            .collect::<Vec<_>>();
        let children = component.as_container().map(|container| {
            container
                .children()
                .iter()
                .flat_map(|child| child.borrow_mut().render(50))
                .map(|line| line.units().to_vec())
                .collect::<Vec<_>>()
        });
        json!({"lines":lines,"children":children,"sameSignal":same,"aborted":signal.aborted(),"reason":reason(&signal),"events":events.lock().unwrap().clone(),"renders":renders.get()})
    };
    let extra = pi_tui::tui::component(pi_tui::Text::new(
        JsString::from_utf16(vec![101, 120, 116, 114, 97, 0xd800]),
        0,
        0,
    ));
    let mut output = vec![snapshot(&mut view)];
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "key" => view.component().handle_input(&raw(&step["units"])),
            "handler" => view.handler(step["tag"].as_str().map(handler)),
            "theme" => set_theme_instance(other.clone()),
            "invalidate" => view.component().invalidate(),
            "dispose" => view.component().dispose(),
            "add" | "remove" | "clear" => {
                let View::Bordered(loader) = &mut view else {
                    panic!("container operation on bare loader");
                };
                match step["op"].as_str().unwrap() {
                    "add" => loader.add_child(extra.clone()),
                    "remove" => loader.remove_child(&extra),
                    "clear" => loader.clear(),
                    _ => unreachable!(),
                }
            }
            op => panic!("unknown operation {op}"),
        }
        output.push(snapshot(&mut view));
    }
    view.component().dispose();
    json!(output)
}
fn key(value: &str) -> Value {
    json!({"op":"key","units":value.encode_utf16().collect::<Vec<_>>()})
}
fn request(bare: bool, cancellable: bool, separate: bool, steps: Vec<Value>) -> Value {
    json!({"bare":bare,"cancellable":cancellable,"separateTheme":separate,"colors":colors(0),"otherColors":colors(1),"message":[119,111,114,107,55296],"steps":steps})
}
#[test]
fn loader_theme_container_and_default_abort_producers_match_frozen() {
    let mut cases = Vec::new();
    for bare in [false, true] {
        for cancellable in [false, true] {
            for separate in [false, true] {
                if bare && !cancellable {
                    continue;
                }
                let steps = vec![
                    key("x"),
                    json!({"op":"theme"}),
                    json!({"op":"invalidate"}),
                    key("\u{1b}"),
                    key("\u{3}"),
                    json!({"op":"handler","tag":"second"}),
                    key("\u{1b}"),
                    json!({"op":"handler","tag":null}),
                    key("\u{3}"),
                    json!({"op":"dispose"}),
                    key("\u{1b}"),
                ];
                cases.push(request(bare, cancellable, separate, steps));
                if !bare {
                    cases.push(request(
                        bare,
                        cancellable,
                        separate,
                        vec![
                            json!({"op":"add"}),
                            json!({"op":"remove"}),
                            json!({"op":"add"}),
                            json!({"op":"clear"}),
                            key("\u{1b}"),
                            json!({"op":"dispose"}),
                        ],
                    ));
                }
            }
        }
    }
    let mut oracle = oracle::TsOracle::spawn("bordered-loader-oracle.mts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for case in cases {
        let expected = oracle.request(&case);
        assert_eq!(expected["ok"], true, "source {expected}");
        let actual = native(&case);
        if actual != expected["output"] {
            let step = actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected["output"].as_array().unwrap())
                .position(|(actual, expected)| actual != expected)
                .unwrap();
            let fields: Vec<_> = [
                "lines",
                "children",
                "sameSignal",
                "aborted",
                "reason",
                "events",
                "renders",
            ]
            .into_iter()
            .filter(|field| actual[step][field] != expected["output"][step][field])
            .collect();
            eprintln!(
                "bare={} cancellable={} separate={} step{step} fields{fields:?}",
                case["bare"], case["cancellable"], case["separateTheme"]
            );
            failures.push(case);
        }
    }
    assert!(failures.is_empty(), "{} bordered loader cases differ", failures.len());
}
