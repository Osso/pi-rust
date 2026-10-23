use pi_js_compat::{
    js_json::JsValue,
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::process::Command;

#[test]
fn native_strings_distinguish_plain_code_properties_from_node_constructor_codes() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/node-error-strings-oracle.cjs");
    let output = Command::new("node")
        .arg("--expose-internals")
        .arg(script)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let factories: Vec<NativeNodeError> = vec![
        NativeNodeError::from_constructor(NativeErrorClass::Error, "fixture")
            .with_property("code", JsValue::String("ERR_FIXTURE".into())),
        NativeNodeError::from_constructor(NativeErrorClass::TypeError, "fixture")
            .with_property("code", JsValue::String("ERR_FIXTURE".into())),
        NativeNodeError::from_node_constructor(NativeErrorClass::TypeError, "ERR_INVALID_ARG_TYPE", "fixture"),
        NativeNodeError::from_node_constructor(NativeErrorClass::RangeError, "ERR_OUT_OF_RANGE", "fixture"),
        pi_js_compat::node_os::homedir_system_error(-5, "EIO", "i/o error"),
    ];
    let mut actual = Vec::new();
    for original in factories {
        let code = original.value()["code"].clone();
        for code in [
            code,
            JsValue::String("CHANGED".into()),
            JsValue::Null,
            JsValue::Number(42.0),
        ] {
            for name in [original.name.clone(), "Renamed".into(), "".into()] {
                for message in ["fixture", ""] {
                    let error = original
                        .clone()
                        .with_property("code", code.clone())
                        .with_property("name", JsValue::String(name.clone()))
                        .with_property("message", JsValue::String(message.into()));
                    actual.push(error.string_value().to_utf8().unwrap());
                }
            }
        }
    }
    assert_eq!(actual, expected);
}
