#[path = "differential_keybindings/mod.rs"]
mod keybindings_oracle;

#[test]
fn keybindings_match_the_pinned_typescript_source() {
    keybindings_oracle::run();
}

/// keybindings.ts:404-408 reads with `readFileSync(path, "utf-8")`: an invalid byte in
/// an unrelated string becomes U+FFFD and the overrides still apply.
#[test]
fn invalid_utf8_bytes_decode_like_node_and_keep_overrides() {
    use pi_coding_agent::core::keybindings::KeybindingsManager;
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("keybindings.json"),
        b"{\"app.interrupt\":\"ctrl+q\",\"_comment\":\"caf\xe9\"}",
    )
    .unwrap();
    let manager = KeybindingsManager::create(Some(&directory.path().to_str().unwrap().into())).unwrap();
    assert_eq!(
        manager.get_keys("app.interrupt"),
        vec![pi_js_compat::js_json::JsString::from("ctrl+q")]
    );
    assert!(manager.matches("\u{11}", "app.interrupt"));
}
