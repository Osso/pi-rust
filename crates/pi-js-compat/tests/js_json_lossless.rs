// Run the existing serde-facing behavioral corpus against the shared implementation.
#[path = "../../pi-coding-agent/tests/js_json.rs"]
mod legacy;

use std::collections::HashSet;

use pi_js_compat::js_json::{JsString, JsValue, parse, parse_lossless, stringify, stringify_lossless};
use serde_json::json;

#[test]
fn strings_keep_units_equality_hash_and_utf16_sort_order() {
    let high = JsString::from_utf16(vec![0xd800]);
    let low = JsString::from_utf16(vec![0xdfff]);
    let replacement = JsString::from("�");
    assert_eq!(high.units(), &[0xd800]);
    assert!(high.to_utf8().is_err());
    assert_eq!(high.to_utf8_lossy(), "�");
    assert_eq!(JsString::from("😀").to_utf8().unwrap(), "😀");
    assert_eq!(JsString::from_utf16(vec![0xd83d, 0xde00]), JsString::from("😀"));
    let keys: HashSet<_> = [high.clone(), low.clone(), replacement.clone(), high.clone()]
        .into_iter()
        .collect();
    assert_eq!(keys.len(), 3);
    let mut sorted = [
        replacement,
        JsString::from("\u{e000}"),
        low.clone(),
        high.clone(),
        JsString::from("😀"),
    ];
    sorted.sort();
    assert_eq!(
        sorted.map(|key| key.units().to_vec()),
        [
            vec![0xd800],
            vec![0xd83d, 0xde00],
            vec![0xdfff],
            vec![0xe000],
            vec![0xfffd]
        ]
    );
    let JsValue::Object(object) = parse_lossless(r#"{"\ud800":1,"\udfff":2,"�":3}"#).unwrap() else {
        panic!("object required")
    };
    assert_eq!(object.get(&high), Some(&JsValue::Number(1.into())));
    assert_eq!(object.get(&low), Some(&JsValue::Number(2.into())));
}

#[test]
fn legacy_parse_replaces_before_collisions_and_preserves_original_slot() {
    let text = r#"{"\ud800":1,"b":0,"�":2,"\ud800":3,"nested":["\udfff"]}"#;
    let legacy = parse(text).unwrap();
    assert_eq!(legacy, json!({"�":3,"b":0,"nested":["�"]}));
    assert_eq!(stringify(&legacy), r#"{"�":3,"b":0,"nested":["�"]}"#);
    // Explicit conversion is intentionally a later UTF8 boundary, not legacy parse.
    assert_eq!(
        parse_lossless(text).unwrap().into_serde_lossy(),
        json!({"�":2,"b":0,"nested":["�"]})
    );
    let numbers = parse("[1e400,-0,9007199254740993,true,false,null]").unwrap();
    assert_eq!(stringify(&numbers), "[null,0,9007199254740992,true,false,null]");
}

#[test]
fn serde_conversion_and_constructed_lone_units_use_shared_writer() {
    let serde = json!({"z":["😀",null,true,1.5],"0":"\u{0}"});
    let value = JsValue::from(&serde);
    assert_eq!(value.clone().into_serde_lossy(), serde);
    assert_eq!(stringify_lossless(&value), stringify(&serde));
    assert_eq!(
        stringify_lossless(&JsValue::String(JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]))),
        r#""\ud800a\udc00""#
    );
}
