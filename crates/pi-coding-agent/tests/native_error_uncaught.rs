use pi_coding_agent::utils::uncaught::format_uncaught;
use pi_js_compat::{
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::Value;

#[test]
fn native_errors_keep_constructor_string_behavior_at_uncaught_boundary() {
    let script = r#"
const {codes:{ERR_SYSTEM_ERROR}}=require('internal/errors');
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const output=[];
for(const system of [false,true]) for(const name of ['Renamed'+String.fromCharCode(0xd800),'']) {
 const error=system?new ERR_SYSTEM_ERROR({errno:-5,code:'EIO',message:'i/o error',syscall:'uv_os_homedir'}):new TypeError('plain message');
 if(!system) error.code='EXAMPLE';
 error.name=name;
 output.push({text:units(String(error)),message:units(error.message),isError:error instanceof Error});
}
console.log(JSON.stringify(output));
"#;
    let source = std::process::Command::new("node")
        .args(["--expose-internals", "-e", script])
        .output()
        .unwrap();
    assert!(source.status.success(), "{}", String::from_utf8(source.stderr).unwrap());
    let expected: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    let mut index = 0;
    for system in [false, true] {
        for name in [
            JsString::from_utf16(vec![82, 101, 110, 97, 109, 101, 100, 0xd800]),
            JsString::default(),
        ] {
            let mut error = if system {
                pi_js_compat::node_os::homedir_system_error(-5, "EIO", "i/o error")
            } else {
                NativeNodeError::from_constructor(NativeErrorClass::TypeError, "plain message")
                    .with_property("code", JsValue::String("EXAMPLE".into()))
            };
            error.name = name;
            assert_eq!(
                serde_json::json!({"text":format_uncaught(&error).units(),"message":error.to_string().encode_utf16().collect::<Vec<_>>(),"isError":error.is_instance_of(NativeErrorClass::Error)}),
                expected[index],
                "system{system}"
            );
            index += 1;
        }
    }
}
