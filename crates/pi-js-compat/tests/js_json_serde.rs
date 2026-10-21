use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};

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
