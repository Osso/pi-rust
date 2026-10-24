use pi_js_compat::js_json::{self, JsValue};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn posix_tmpdir_matches_node_26_environment_priority_and_single_slash_strip() {
    let cases: Vec<BTreeMap<String, String>> = vec![
        BTreeMap::new(),
        BTreeMap::from([
            ("TMPDIR".into(), "".into()),
            ("TMP".into(), "from-tmp".into()),
            ("TEMP".into(), "from-temp".into()),
        ]),
        BTreeMap::from([
            ("TMPDIR".into(), "from-tmpdir".into()),
            ("TMP".into(), "from-tmp".into()),
        ]),
        BTreeMap::from([("TEMP".into(), "from-temp/".into())]),
        BTreeMap::from([("TMPDIR".into(), "/".into())]),
        BTreeMap::from([("TMPDIR".into(), "//".into())]),
        BTreeMap::from([("TMPDIR".into(), "///".into())]),
        BTreeMap::from([("TMPDIR".into(), "relative///".into())]),
        BTreeMap::from([("TMPDIR".into(), "é😀/".into())]),
        BTreeMap::from([("TMPDIR".into(), "backslash\\".into())]),
        BTreeMap::from([
            ("TMPDIR".into(), "".into()),
            ("TMP".into(), "".into()),
            ("TEMP".into(), "temp//".into()),
        ]),
    ];
    let script = "const fs=require('node:fs'),os=require('node:os');const cases=JSON.parse(fs.readFileSync(0,'utf8'));const output=cases.map(env=>{for(const key of ['TMPDIR','TMP','TEMP'])delete process.env[key];Object.assign(process.env,env);return os.tmpdir()});process.stdout.write(JSON.stringify({version:process.version,output}));";
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(js_json::stringify_lossless(&js_json::to_value(&cases).unwrap()).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected = js_json::parse_lossless(String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(expected["version"], pi_js_compat::js_json!("v26.11.1"));
    for (index, env) in cases.iter().enumerate() {
        for key in ["TMPDIR", "TMP", "TEMP"] {
            pi_js_compat::process_env::remove(key);
        }
        for (key, value) in env {
            pi_js_compat::process_env::set(key, value);
        }
        let actual = JsValue::String(pi_js_compat::node_os::tmpdir());
        assert_eq!(actual, expected["output"][index], "case {index}");
    }
    pi_js_compat::process_env::clear();
}
