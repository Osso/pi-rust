//! Supervisor project resolution keeps JavaScript string units through config, cwd and remotes.

use pi_coding_agent::supervisor::project_resolver::{
    SupervisorProjectConfig, extract_remote_repository_identity, read_supervisor_project_config,
    resolve_supervisor_project,
};
use pi_js_compat::js_json::JsString;

fn units(prefix: &str, unit: u16) -> JsString {
    let mut text: Vec<u16> = prefix.encode_utf16().collect();
    text.push(unit);
    JsString::from_utf16(text)
}

#[test]
fn cwd_basename_fallback_keeps_unpaired_units() {
    let cwd = units("/repos/p", 0xd800);
    let project = resolve_supervisor_project(&SupervisorProjectConfig::new(), &cwd, &[]).unwrap();
    assert_eq!(project, units("p", 0xd800));
}

#[test]
fn remote_identity_keeps_unpaired_units() {
    let remote = JsString::from_utf16(
        "git@host:owner/r"
            .encode_utf16()
            .chain([0xdc00])
            .chain(".git".encode_utf16())
            .collect(),
    );
    assert_eq!(
        extract_remote_repository_identity(&remote),
        Some(units("owner/r", 0xdc00))
    );
}

#[test]
fn escaped_config_keys_and_roots_match_raw_cwd() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projects.json");
    std::fs::write(&path, r#"{"proj\ud800":{"roots":["/work/r\udc00"]}}"#).unwrap();
    let config = read_supervisor_project_config(&path.to_str().unwrap().into()).unwrap();
    let cwd = JsString::from_utf16(
        "/work/r"
            .encode_utf16()
            .chain([0xdc00])
            .chain("/sub".encode_utf16())
            .collect(),
    );
    let project = resolve_supervisor_project(&config, &cwd, &[]).unwrap();
    assert_eq!(project, units("proj", 0xd800));
}
