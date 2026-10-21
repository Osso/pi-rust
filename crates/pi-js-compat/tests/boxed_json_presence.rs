use pi_js_compat::js_json::JsValue;

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Record {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    details: Option<Box<JsValue>>,
}

#[test]
fn boxed_details_keep_absence_null_and_raw_values_distinct() {
    for wire in [r#"{}"#, r#"{"details":null}"#, r#"{"details":{"\ud800":"\udfff"}}"#] {
        let record: Record = serde_json::from_str(wire).unwrap();
        assert_eq!(serde_json::to_string(&record).unwrap(), wire);
    }
}
