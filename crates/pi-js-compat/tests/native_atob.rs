use pi_js_compat::{base64::atob, js_json::JsString};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn atob_retains_native_units_and_matches_node_errors() {
    let mut inputs = vec![
        vec![0x59, 0x20, 0x51, 0x09, 0x3d, 0x0a, 0x3d, 0x0c, 0x0d],
        vec![0xd800],
        vec![0xdc00],
        vec![0x100],
        vec![0xff],
    ];
    let alphabet = [0x41, 0x3d, 0x21, 0x20, 0xd800];
    for length in 0..=5_u32 {
        for mut index in 0..alphabet.len().pow(length) {
            let mut units = Vec::new();
            for _ in 0..length {
                units.push(alphabet[index % alphabet.len()]);
                index /= alphabet.len();
            }
            inputs.push(units);
        }
    }
    let mut child = Command::new("node")
        .args(["-e", "let s='';process.stdin.on('data',x=>s+=x);process.stdin.on('end',()=>console.log(JSON.stringify(JSON.parse(s).map(u=>{try{return {bytes:[...atob(String.fromCharCode(...u))].map(c=>c.charCodeAt(0))}}catch(e){return {name:e.name,message:e.message}}}))));"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&inputs).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (units, expected) in inputs.into_iter().zip(expected) {
        let input = JsString::from_utf16(units);
        let actual = match atob(&input) {
            Ok(bytes) => json!({"bytes": bytes}),
            Err(error) => json!({"name": error.name(), "message": error.message()}),
        };
        assert_eq!(actual, expected, "input units: {:?}", input.units());
    }
}
