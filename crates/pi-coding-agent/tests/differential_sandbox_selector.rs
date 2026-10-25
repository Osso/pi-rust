use crate::shared_oracle as oracle;

use pi_coding_agent::{
    core::permissions::presets::SandboxProfileName,
    modes::interactive::{
        components::sandbox_selector::{SandboxSelectorComponent, SandboxSelectorOptions, SandboxSelectorSelection},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn initialize_theme() {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).into(), ColorValue::Index(0.0)))
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
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
fn native(request: &Value) -> Value {
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let selected = events.clone();
    let cancelled = events.clone();
    let mut component = SandboxSelectorComponent::new_for_platform(
        SandboxSelectorOptions {
            current_profile: SandboxProfileName::parse(request["current"].as_str().unwrap()).unwrap(),
            session_profile: request["session"].as_str().and_then(SandboxProfileName::parse),
            on_select: Rc::new(move |selection| {
                let (profile, scope) = match selection {
                    SandboxSelectorSelection::Saved { profile, scope } => (profile.as_str(), scope.as_str()),
                    SandboxSelectorSelection::Session { profile } => {
                        (profile.map_or("none", SandboxProfileName::as_str), "session")
                    }
                };
                selected.borrow_mut().push(format!("select:{profile}:{scope}"));
            }),
            on_cancel: Rc::new(move || cancelled.borrow_mut().push("cancel".into())),
        },
        request["platform"].as_str().unwrap(),
    );
    let width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |component: &mut SandboxSelectorComponent| {
        let lines: Vec<_> = component
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect();
        let children: Vec<_> = component.as_container().map_or_else(Vec::new, |container| {
            container
                .children()
                .iter()
                .flat_map(|child| child.borrow_mut().render(width))
                .map(|line| line.units().to_vec())
                .collect()
        });
        json!({"lines":lines,"children":children,"events":events.borrow().clone()})
    };
    let mut output = vec![snapshot(&mut component)];
    for key in request["keys"].as_array().unwrap() {
        component.handle_input(&JsString::from(key.as_str().unwrap()));
        component.invalidate();
        output.push(snapshot(&mut component));
    }
    let extra = pi_tui::tui::component(InvalidatingChild::default());
    component.add_child(extra.clone());
    output.push(snapshot(&mut component));
    component.invalidate();
    output.push(snapshot(&mut component));
    component.remove_child(&extra);
    output.push(snapshot(&mut component));
    component.clear();
    output.push(snapshot(&mut component));
    component.handle_input(&JsString::from("\t"));
    output.push(snapshot(&mut component));
    json!(output)
}
#[test]
fn sandbox_scopes_clamping_callbacks_and_inherited_children_match_frozen() {
    initialize_theme();
    let mut oracle = oracle::TsOracle::spawn("sandbox-selector-oracle.ts", "packages/coding-agent/src");
    for platform in ["linux", "darwin", "win32", "freebsd"] {
        for current in ["read-only", "workspace-write", "full-access"] {
            for session in [None, Some("read-only"), Some("workspace-write"), Some("full-access")] {
                for width in [12, 100] {
                    let request = json!({"platform":platform,"current":current,"session":session,"width":width,"keys":["k","k","k","\r","j","j","j","j","\n","\t","\r","\t","\r","j","\r","\t","\r","\u{1b}","\u{1b}[A","\u{1b}[B","x"]});
                    let expected = oracle.request(&request);
                    assert_eq!(expected["ok"], true, "oracle: {expected}");
                    let actual = native(&request);
                    assert_eq!(
                        actual.as_array().unwrap().len(),
                        expected["output"].as_array().unwrap().len()
                    );
                    for (index, (actual, expected)) in actual
                        .as_array()
                        .unwrap()
                        .iter()
                        .zip(expected["output"].as_array().unwrap())
                        .enumerate()
                    {
                        for field in ["lines", "children", "events"] {
                            assert_eq!(
                                actual[field], expected[field],
                                "request {request}, step {index}, field {field}"
                            );
                        }
                    }
                }
            }
        }
    }
}
