use crate::shared_oracle as oracle;
use pi_coding_agent::utils::git::parse_git_url;
use pi_js_compat::js_json::JsString;
#[test]
fn generic_git_reference_retains_unpaired_units() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let source = JsString::from("git:git@private.example:user/repo@") + &raw;
    let parsed = parse_git_url(&source).unwrap();
    assert_eq!(parsed.repo, JsString::from("git@private.example:user/repo"));
    assert_eq!(parsed.reference, Some(raw));
    assert_eq!(parsed.path, JsString::from("user/repo"));
    let mut oracle = oracle::TsOracle::spawn("coding-agent-git-oracle.ts", "packages/coding-agent/src/utils/git.ts");
    let expected = oracle.request(&pi_js_compat::js_json!({"id":1,"cases":[{"name":"raw-reference","source":source}]}));
    assert_eq!(expected["ok"].as_bool(), Some(true));
    assert_eq!(
        pi_js_compat::js_json::to_value(&parsed).unwrap(),
        expected["output"]["results"].as_array().unwrap()[0]["value"]
    );
}
