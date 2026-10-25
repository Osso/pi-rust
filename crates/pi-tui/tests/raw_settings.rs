use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::settings_list::{SettingItem, SettingsList, SettingsListOptions, SettingsListTheme};
use pi_tui::tui::Component;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[test]
fn settings_native_ids_values_search_and_change_callbacks_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![97, 0xd800, 98, 0xdc00],
    ] {
        for width in [1, 10, 50] {
            for search in [false, true] {
                let expected =
                    oracle.request(&json!({"operation":"settings","units":units,"width":width,"search":search}));
                assert_eq!(expected["ok"], true, "{expected}");
                let raw = JsString::from_utf16(units.clone());
                let changes = Rc::new(RefCell::new(Vec::new()));
                let captured = changes.clone();
                let cancelled = Rc::new(RefCell::new(0));
                let cancelled_value = cancelled.clone();
                let items = (0..2)
                    .map(|index| SettingItem {
                        id: raw.clone() + index.to_string(),
                        label: raw.clone() + "label" + index.to_string(),
                        description: Some(raw.clone() + " desc"),
                        current_value: raw.clone() + "v0",
                        values: Some(vec![raw.clone() + "v0", raw.clone() + "v1"]),
                        submenu: None,
                    })
                    .collect();
                let mut list = SettingsList::new(
                    items,
                    2,
                    SettingsListTheme::plain("→ "),
                    Rc::new(move |id, value| captured.borrow_mut().push((id.clone(), value.clone()))),
                    Rc::new(move || *cancelled_value.borrow_mut() += 1),
                    SettingsListOptions { enable_search: search },
                );
                let mut renders = vec![list.render(width)];
                list.update_value(&(raw.clone() + "0"), raw.clone() + "external");
                renders.push(list.render(width));
                for key in [
                    JsString::from("\r"),
                    raw.clone(),
                    JsString::from("\x1b[B"),
                    JsString::from("\r"),
                    JsString::from("\x1b"),
                ] {
                    list.handle_input(&key);
                    renders.push(list.render(width));
                }
                let actual = serde_json::to_string(&(
                    renders,
                    changes.borrow().clone(),
                    *cancelled.borrow(),
                    list.selected_index(),
                ))
                .unwrap();
                assert_eq!(
                    parse_lossless(actual).unwrap(),
                    parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                    "units={units:?} width={width} search={search}"
                );
            }
        }
    }
}
