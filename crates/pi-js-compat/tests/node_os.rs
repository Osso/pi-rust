use pi_js_compat::{
    js_json::{JsString, JsValue},
    node_error::NativeErrorClass,
    node_os::{homedir, homedir_system_error},
};
use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Command};

#[test]
fn homedir_matches_node_environment_and_passwd_boundaries() {
    for home in [
        Some(OsString::from("/tmp/native-home")),
        Some(OsString::new()),
        None,
        Some(OsString::from_vec(b"/tmp/\xff\xed\xa0\x80".to_vec())),
    ] {
        let mut native = Command::new(std::env::current_exe().unwrap());
        native.args(["--ignored", "--exact", "homedir_child", "--nocapture"]);
        let mut source = Command::new("node");
        source.args(["-e","const s=require('os').homedir();console.log('RESULT:'+JSON.stringify(Array.from({length:s.length},(_,i)=>s.charCodeAt(i))))"]);
        for command in [&mut native, &mut source] {
            command.env("USERPROFILE", "/not-the-posix-home");
            if let Some(home) = &home {
                command.env("HOME", home);
            } else {
                command.env_remove("HOME");
            }
        }
        let result = |mut command: Command| {
            let output = command.output().unwrap();
            assert!(output.status.success(), "{:?}", output);
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("RESULT:"))
                .unwrap()
                .to_owned()
        };
        assert_eq!(result(native), result(source), "HOME {home:?}");
    }
}
#[test]
#[ignore = "isolated environment child"]
fn homedir_child() {
    println!("RESULT:{}", serde_json::to_string(homedir().unwrap().units()).unwrap());
}

#[test]
fn homedir_system_error_matches_node_error_data() {
    for (errno, code, message) in [(-5, "EIO", "i/o error"), (-2, "ENOENT", "no such file or directory")] {
        let script = format!(
            "const E=require('internal/errors').codes.ERR_SYSTEM_ERROR;const e=new E({{errno:{errno},code:'{code}',message:'{message}',syscall:'uv_os_homedir'}});console.log(JSON.stringify({{text:String(e),message:e.message,code:e.code,info:e.info,errno:e.errno,syscall:e.syscall}}))"
        );
        let output = Command::new("node")
            .args(["--expose-internals", "-e", &script])
            .output()
            .unwrap();
        assert!(output.status.success());
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let error = homedir_system_error(errno, code, message);
        assert_eq!(error.constructor_class(), NativeErrorClass::SystemError);
        let JsValue::Object(value) = error.value() else {
            panic!()
        };
        let mut actual = serde_json::Map::new();
        for key in ["message", "code", "info", "errno", "syscall"] {
            actual.insert(
                key.into(),
                serde_json::from_str(&pi_js_compat::js_json::stringify_lossless(
                    value.get(&JsString::from(key)).unwrap(),
                ))
                .unwrap(),
            );
        }
        actual.insert(
            "text".into(),
            serde_json::Value::String(pi_js_compat::js_json::encode_node_utf8(&error.string_value())),
        );
        assert_eq!(serde_json::Value::Object(actual), expected);
    }
}
