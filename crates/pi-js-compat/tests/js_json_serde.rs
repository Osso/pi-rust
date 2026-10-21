use pi_js_compat::js_json::{JsObject, JsString, JsValue, parse_lossless, stringify_lossless};

#[test]
fn utf16_helpers_and_node_write_boundary_preserve_distinct_authorities() {
    use pi_js_compat::js_json::encode_node_utf8;
    let high = JsString::from_utf16(vec![0xd800]);
    let low = JsString::from_utf16(vec![0xdfff]);
    let joined = JsString::join([&high, &low], "|");
    assert_eq!(joined.units(), &[0xd800, 0x7c, 0xdfff]);
    assert_eq!(encode_node_utf8(&joined), "�|�");
    assert_eq!(joined.units(), &[0xd800, 0x7c, 0xdfff]);
    let trimmed = JsString::join(
        [JsString::from("\u{feff} "), high.clone(), JsString::from("\u{2029}")],
        "",
    )
    .trim();
    assert_eq!(trimmed, high);
    assert!(joined.contains(&high));
    assert!(joined.starts_with(&high));
    assert!(joined.ends_with(&low));
    assert_eq!(joined.split('|').collect::<Vec<_>>(), vec![high, low]);
}

#[test]
fn object_fields_and_scalar_serde_value_inputs_are_exact() {
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Record {
        #[serde(
            serialize_with = "pi_js_compat::js_json::serialize_object",
            deserialize_with = "pi_js_compat::js_json::deserialize_object"
        )]
        args: JsObject,
    }
    let source = r#"{"args":{"ascii":1,"\ud800":"\udfff"}}"#;
    let record: Record = serde_json::from_str(source).unwrap();
    assert_eq!(record.args.get("ascii"), Some(&JsValue::Number(1.0)));
    assert!(record.args.contains_key(&JsString::from_utf16(vec![0xd800])));
    assert_eq!(serde_json::to_string(&record).unwrap(), source);
    let value = serde_json::json!({"text":"😀"});
    let actual: JsValue = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(actual, JsValue::from(value));
}

#[test]
fn lossless_json_macro_builds_nested_values_without_serde_value_projection() {
    let key = JsString::from_utf16(vec![0xd800]);
    let text = JsString::from_utf16(vec![0xdfff]);
    let value = pi_js_compat::js_json!({ (key): [text, {"null":null,"bool":true}], "number": 1.0 });
    assert_eq!(
        stringify_lossless(&value),
        r#"{"\ud800":["\udfff",{"null":null,"bool":true}],"number":1}"#
    );
}

#[test]
fn serde_preserves_utf16_strings_keys_and_nested_values() {
    let source = r#"{"\ud800":["\udfff",{"text":"a\ud800b","null":null}],"pair":"\ud83d\ude00"}"#;
    let expected = parse_lossless(source).unwrap();
    let actual: JsValue = serde_json::from_str(source).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(serde_json::to_string(&actual).unwrap(), stringify_lossless(&expected));
    let decoded: JsValue = serde_json::from_str(&serde_json::to_string(&actual).unwrap()).unwrap();
    assert_eq!(decoded, expected);
}

#[test]
fn serde_string_does_not_replace_lone_surrogates() {
    let text: JsString = serde_json::from_str(r#""\ud800x\udfff""#).unwrap();
    assert_eq!(text.units(), &[0xd800, 0x78, 0xdfff]);
    assert_eq!(serde_json::to_string(&text).unwrap(), r#""\ud800x\udfff""#);
    assert!(text.to_utf8().is_err());
}

#[test]
fn serde_raw_fragments_work_inside_structs_and_preserve_null_presence() {
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct ResultValue {
        text: JsString,
        #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
        details: Option<JsValue>,
    }
    fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<JsValue>, D::Error> {
        JsValue::deserialize(d).map(Some)
    }
    use serde::Deserialize;
    for source in [
        r#"{"text":"\ud800"}"#,
        r#"{"text":"\ud800","details":null}"#,
        r#"{"text":"\ud800","details":{"x":"\udfff"}}"#,
    ] {
        let result: ResultValue = serde_json::from_str(source).unwrap();
        assert_eq!(serde_json::to_string(&result).unwrap(), source);
    }
}
