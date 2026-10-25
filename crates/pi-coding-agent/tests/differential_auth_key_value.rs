use crate::shared_oracle as oracle;
use pi_coding_agent::core::auth_storage::{AuthStorage, GetApiKeyOptions};
use pi_js_compat::{
    js_json::{self, JsString, JsValue},
    node_error::NativeErrorClass,
};

#[tokio::test]
async fn persisted_api_key_values_keep_node_resolution_and_type_errors() {
    let mut source = oracle::TsOracle::spawn(
        "auth-key-value-oracle.mts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    for credential in [
        r#"{"type":"api_key"}"#,
        r#"{"type":"api_key","key":null}"#,
        r#"{"type":"api_key","key":false}"#,
        r#"{"type":"api_key","key":123.5}"#,
        r#"{"type":"api_key","key":[]}"#,
        r#"{"type":"api_key","key":{}}"#,
        r#"{"type":"api_key","key":{"startsWith":true}}"#,
        r#"{"type":"api_key","key":""}"#,
        r#"{"type":"api_key","key":"fixture\ud800"}"#,
        r#"{"type":"api_key","key":"$Q15_KEY","env":{"Q15_KEY":"fixture\udc00"}}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("auth.json");
        let credential = js_json::parse_lossless(credential).unwrap();
        std::fs::write(
            &path,
            js_json::stringify_lossless(&pi_js_compat::js_json!({"anthropic":credential})),
        )
        .unwrap();
        let expected = source.request(&pi_js_compat::js_json!({"id":1,"path":path.to_str().unwrap()}));
        assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
        let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
        let actual = match storage.get_api_key("anthropic", GetApiKeyOptions::default()).await {
            Ok(value) => {
                pi_js_compat::js_json!({"ok":true,"value":value.map(JsValue::String).unwrap_or(JsValue::Null)})
            }
            Err(error) => {
                let thrown = error.thrown_value().as_error().unwrap();
                pi_js_compat::js_json!({"ok":false,"name":thrown.name.clone(),"message":thrown.message.clone(),"constructor":thrown.constructor_class().name(),"typeError":thrown.is_instance_of(NativeErrorClass::TypeError)})
            }
        };
        assert_eq!(&actual, expected.get("output").unwrap(), "credential={credential:?}");
    }
}
