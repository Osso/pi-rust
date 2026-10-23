use pi_js_compat::js_json::{self, JsString};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn spawn_argument_rejections_match_node_before_exec() {
    let cases = [
        (JsString::from(""), vec![]),
        (JsString::from("bad\0file"), vec!["bad\0argument".into()]),
        (
            JsString::from("/bin/true"),
            vec!["first".into(), "second\0argument".into()],
        ),
        (
            JsString::from("/bin/true"),
            vec![JsString::from("x".repeat(115)) + JsString::from_utf16(vec![0xd800, 0]) + "tail"],
        ),
        (JsString::from("/bin/true"), vec![JsString::from_utf16(vec![0xd800])]),
        (JsString::from("/missing/unused"), vec![]),
    ];
    for (file, args) in cases {
        let request = pi_js_compat::js_json!({"file":file,"args":args});
        let mut child = Command::new("node")
            .args([
                "--input-type=commonjs",
                "-e",
                r#"
const {spawnSync} = require('node:child_process');
const request = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
try {
    spawnSync(request.file, request.args, {stdio:'ignore'});
    console.log(JSON.stringify({ok:true}));
} catch (error) {
    const value = {name:error.name, message:error.message};
    for (const key of Object.getOwnPropertyNames(error)) {
        if (!['stack','name','message'].includes(key)) value[key] = error[key];
    }
    console.log(JSON.stringify({error:value, constructor:error.constructor.name, text:String(error)}));
}
"#,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(js_json::stringify_lossless(&request).as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let expected = js_json::parse_lossless(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
        let actual = match pi_js_compat::node_child_process::validate_spawn_arguments(&file, &args) {
            Ok(()) => pi_js_compat::js_json!({"ok":true}),
            Err(error) => pi_js_compat::js_json!({
                "error":error.value(), "constructor":error.constructor_class().name(), "text":error.string_value()
            }),
        };
        assert_eq!(actual, expected, "{}", js_json::stringify_lossless(&request));
    }
}
