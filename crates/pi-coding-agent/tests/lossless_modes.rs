use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use pi_coding_agent::modes::interactive::components::diff::render_diff;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8, parse_lossless, stringify_lossless};

fn install_oracle_theme() {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap();
    set_theme_instance(Arc::new(theme));
}

fn render_with_frozen_typescript(cases: &[JsString]) -> Vec<JsString> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: serde_json::Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    let source = PathBuf::from(lock["worktree"].as_str().unwrap());
    let head = Command::new("git")
        .args(["-C", source.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        lock["commit"].as_str().unwrap()
    );
    let mut child = Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/interactive-components-oracle.ts"))
        .arg(source.join("packages/coding-agent/src"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let request = pi_js_compat::js_json!({
        "id": 1,
        "module": "modes/interactive/components/diff.ts",
        "operation": "renderDiff",
        "cases": cases,
    });
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", stringify_lossless(&request)).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response = parse_lossless(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
    response["output"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_string().unwrap().clone())
        .collect()
}

#[test]
fn raw_diff_styling_matches_frozen_utf16_then_node_terminal_encoding() {
    // Color detection is process-scoped. Set it before either implementation loads.
    if std::env::var_os("PI_LOSSLESS_DIFF_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "raw_diff_styling_matches_frozen_utf16_then_node_terminal_encoding",
                "--nocapture",
            ])
            .env("PI_LOSSLESS_DIFF_CHILD", "1")
            .env("FORCE_COLOR", "3")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    install_oracle_theme();
    let cases = [
        vec![45, 49, 32, 97, 0xd800, 98, 10, 43, 49, 32, 97, 0xd801, 98],
        vec![45, 49, 32, 0xd800, 10, 43, 49, 32, 0xfffd],
        vec![45, 49, 32, 0xd83d, 0xde00, 10, 43, 49, 32, 0xd83d, 0xde01],
        vec![45, 49, 32, 9, 0xd800, 10, 43, 49, 32, 9, 0xd801],
        vec![32, 49, 32, 0xd800, 10, 32, 50, 32, 0xd83d, 0xde00],
        vec![45, 49, 32, 0xd800, 13, 10, 43, 49, 32, 0xd801, 13],
        vec![45, 49, 32, 0xd800, 0x2028, 10, 43, 49, 32, 0xd801, 0x2028],
        vec![45, 49, 32, 0xd800, 10, 45, 50, 32, 0xd801, 10, 43, 49, 32, 0xfffd],
    ]
    .into_iter()
    .map(JsString::from_utf16)
    .collect::<Vec<_>>();
    let expected = render_with_frozen_typescript(&cases);
    for (input, expected) in cases.iter().zip(expected) {
        assert_eq!(
            render_diff(input),
            encode_node_utf8(&expected),
            "raw input: {:?}",
            input.units()
        );
    }
}
