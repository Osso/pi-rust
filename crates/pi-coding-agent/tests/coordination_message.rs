use crate::shared_oracle as oracle;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{CustomMessageContent, CustomMessageValue};
use pi_coding_agent::modes::interactive::components::coordination_message::CoordinationMessageComponent;
use pi_coding_agent::modes::interactive::components::custom_message::CustomMessageComponent;
use pi_coding_agent::modes::interactive::components::user_message::UserMessageComponent;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_js_compat::js_json::JsString;
use pi_tui::tui::{ComponentRef, component};
use pi_tui::{Component, Text};
use serde_json::json;

fn install_theme() {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(
                    format!(
                        "#{:02x}{:02x}{:02x}",
                        (16 + 7 * i) % 256,
                        (32 + 11 * i) % 256,
                        (48 + 13 * i) % 256
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

#[test]
fn hidden_coordination_component_matches_frozen_typescript() {
    install_theme();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for rich in [false, true] {
        let states = [
            (false, false, false),
            (true, true, true),
            (true, false, false),
            (false, true, true),
            (false, false, false),
        ];
        let expected = oracle.request(&json!({
            "id":"hidden", "module":"modes/interactive/components/coordination-message.ts",
            "operation":"coordinationComponent", "text":"body **markdown**", "width":40,"rich":rich,
            "states":states.iter().map(|(hidden,expanded,in_transit)|json!({"hidden":hidden,"expanded":expanded,"inTransit":in_transit})).collect::<Vec<_>>(),
        }));
        assert_eq!(expected["ok"], true, "{expected}");
        let hidden = Rc::new(Cell::new(false));
        let callback = {
            let hidden = Rc::clone(&hidden);
            Rc::new(move || hidden.get())
        };
        let children: Vec<ComponentRef> = if rich {
            vec![
                component(UserMessageComponent::new("body **markdown**", None, 1)),
                component(CustomMessageComponent::new(
                    CustomMessageValue {
                        custom_type: "shared_channel".into(),
                        content: CustomMessageContent::Text("custom".into()),
                        display: true,
                        details: None,
                        timestamp: 0.0,
                    },
                    Some(Arc::new(|_, options, _| {
                        Some(component(Text::new(format!("expanded:{}", options.expanded), 0, 0)))
                    })),
                    None,
                )),
                component(Text::new("ordinary", 0, 0)),
            ]
        } else {
            vec![component(Text::new("body **markdown**", 0, 0))]
        };
        let mut component = CoordinationMessageComponent::new(children, callback);
        let mut frames = Vec::new();
        for (is_hidden, expanded, in_transit) in states {
            hidden.set(is_hidden);
            component.set_expanded(expanded);
            component.set_in_transit(in_transit);
            frames.push(component.render(40));
        }
        assert_eq!(json!(frames), expected["output"]);
    }
}

#[test]
fn raw_children_survive_hiding_without_replacement() {
    let raw = JsString::from_utf16(vec![0xd800, b'x' as u16, 0xdc00]);
    let child = component(Text::new(raw.clone(), 0, 0));
    let hidden = Rc::new(Cell::new(false));
    let callback = {
        let hidden = Rc::clone(&hidden);
        Rc::new(move || hidden.get())
    };
    let mut component = CoordinationMessageComponent::new(vec![Rc::clone(&child)], callback);
    let before = component.render(20);
    hidden.set(true);
    assert!(component.render(20).is_empty());
    component.invalidate();
    hidden.set(false);
    assert_eq!(component.render(20), before);
    assert!(Rc::ptr_eq(&component.children()[0], &child));
    assert!(before[0].contains(&raw));
}
