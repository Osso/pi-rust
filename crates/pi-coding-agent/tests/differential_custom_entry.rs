use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_manager::CustomEntry;
use pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponent;
use pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponentOptions;
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use pi_tui::tui::{ComponentRef, component};
use serde_json::Value;
use serde_json::json;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn install_theme() {
    use pi_coding_agent::modes::interactive::theme::{
        ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance,
    };
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        (16 + 7 * index) % 256,
                        (32 + 11 * index) % 256,
                        (48 + 13 * index) % 256
                    )
                    .into(),
                ),
            )
        })
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap(),
    ));
}

struct Generation {
    id: usize,
    events: Arc<Mutex<Vec<Value>>>,
}
impl Component for Generation {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        vec![format!("generation {}", self.id).into()]
    }
    fn invalidate(&mut self) {
        self.events.lock().unwrap().push(json!(["invalidate", self.id]));
    }
    fn dispose(&mut self) {
        self.events.lock().unwrap().push(json!(["dispose", self.id]));
    }
}

#[test]
fn custom_entry_replacement_cleanup_and_disposal_match_frozen() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for empty in [false, true] {
        let request = json!({"id":2,"module":"modes/interactive/components/custom-entry.ts","operation":"customEntryLifecycle","customType":"fixture","sessionId":"session","empty":empty,"width":40});
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let generation = Arc::new(Mutex::new(0usize));
        let renderer = {
            let events = events.clone();
            Arc::new(
                move |_: &CustomEntry,
                      options: &pi_coding_agent::EntryRenderOptions,
                      _: &pi_coding_agent::modes::interactive::theme::Theme|
                      -> Option<ComponentRef> {
                    let current = {
                        let mut value = generation.lock().unwrap();
                        *value += 1;
                        *value
                    };
                    events
                        .lock()
                        .unwrap()
                        .push(json!(["build", current, options.expanded, options.session_id]));
                    let requested = (options.request_render)().unwrap();
                    events.lock().unwrap().push(json!(["requested", requested]));
                    let cleanup: Rc<dyn Fn()> = {
                        let events = events.clone();
                        Rc::new(move || events.lock().unwrap().push(json!(["cleanup", current])))
                    };
                    (options.register_cleanup)(cleanup.clone());
                    (options.register_cleanup)(cleanup);
                    (!empty).then(|| {
                        component(Generation {
                            id: current,
                            events: events.clone(),
                        })
                    })
                },
            )
        };
        let entry = CustomEntry {
            custom_type: "fixture".into(),
            data: None,
            id: "entry".into(),
            parent_id: None,
            timestamp: "2026-01-01".into(),
        };
        let request_render = {
            let events = events.clone();
            Rc::new(move || {
                events.lock().unwrap().push(json!(["request"]));
                Ok(true)
            })
        };
        let mut component = CustomEntryComponent::new(
            entry,
            renderer,
            CustomEntryComponentOptions {
                session_id: "session".into(),
                request_render,
            },
        );
        let mut frames = vec![component.render(40)];
        component.set_expanded(true);
        frames.push(component.render(40));
        component.set_expanded(true);
        component.invalidate();
        frames.push(component.render(40));
        component.dispose();
        component.dispose();
        let frames: Vec<Vec<Vec<u16>>> = frames
            .iter()
            .map(|lines| lines.iter().map(|line| line.units().to_vec()).collect())
            .collect();
        assert_eq!(
            json!({"frames":frames,"events":*events.lock().unwrap(),"hasContent":component.has_content(),"retired":component.render(40)}),
            expected["output"]
        );
    }
}

#[test]
fn custom_entry_renderer_error_matches_frozen() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let error = JsString::from_utf16(vec![0xd800, 0x78, 0xdc00]);
    let request = json!({"id":1,"module":"modes/interactive/components/custom-entry.ts","operation":"customEntryLifecycle","throws":true,"customType":"fixture","sessionId":"session","errorUnits":error.units(),"width":40});
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    let entry = CustomEntry {
        custom_type: "fixture".into(),
        data: None,
        id: "entry".into(),
        parent_id: None,
        timestamp: "2026-01-01".into(),
    };
    let renderer = Arc::new(
        move |_: &CustomEntry,
              _: &pi_coding_agent::EntryRenderOptions,
              _: &pi_coding_agent::modes::interactive::theme::Theme|
              -> Option<pi_tui::tui::ComponentRef> {
            pi_coding_agent::utils::uncaught::throw_uncaught("Error", error.clone());
        },
    );
    let mut component = CustomEntryComponent::new(
        entry,
        renderer,
        CustomEntryComponentOptions {
            session_id: "session".into(),
            request_render: Rc::new(|| Ok(false)),
        },
    );
    let lines: Vec<_> = component.render(40).iter().map(|line| line.units().to_vec()).collect();
    assert_eq!(json!(lines), expected["output"]["frames"][0]);
}
