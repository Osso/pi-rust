use crate::shared_oracle as oracle;

use std::{cell::RefCell, rc::Rc};

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme};
use pi_tui::tui::Component;
use serde_json::json;

#[test]
fn selection_native_labels_descriptions_filter_and_callbacks_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0x61, 0xd800, 0x62, 0xdc00],
    ] {
        for width in [0, 1, 4, 12, 41, 72] {
            let response = oracle.request(&json!({"operation":"select", "units":units,"width":width}));
            assert_eq!(response["ok"], true, "{response}");
            let raw = JsString::from_utf16(units.clone());
            let items = (0..3)
                .map(|index| {
                    SelectItem::new(
                        raw.clone() + index.to_string(),
                        raw.clone() + "label" + index.to_string(),
                    )
                    .with_description(raw.clone() + "\r\n desc " + raw.clone())
                })
                .collect();
            let mut list = SelectList::new(items, 2, SelectListTheme::plain(), SelectListLayoutOptions::default());
            let callbacks = Rc::new(RefCell::new(Vec::new()));
            let values = callbacks.clone();
            list.on_select = Some(Rc::new(move |item| values.borrow_mut().push(item.value.clone())));
            let mut lines = vec![list.render(width)];
            list.handle_input(&JsString::from("\x1b[B"));
            list.handle_input(&JsString::from("\r"));
            lines.push(list.render(width));
            list.set_filter(&raw);
            lines.push(list.render(width));
            let selected = list.get_selected_item().map(|item| item.value.clone());
            list.set_filter(&(raw.clone() + "missing"));
            lines.push(list.render(width));
            let actual = serde_json::to_string(&(lines, selected, callbacks.borrow().clone())).unwrap();
            assert_eq!(
                parse_lossless(actual).unwrap(),
                parse_lossless(response["json"].as_str().unwrap()).unwrap(),
                "units={units:?} width={width}"
            );
        }
    }
}
