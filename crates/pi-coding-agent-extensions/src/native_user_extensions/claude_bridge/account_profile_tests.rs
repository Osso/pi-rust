use super::account_profiles::*;
use pi_js_compat::js_json::{self, JsString};
use std::os::unix::fs::{MetadataExt, symlink};

const OLD: &str = r#"{"claudeAiOauth":{"accessToken":"old","refreshToken":"old-refresh"},"note":"\udfff"}"#;
const NEW: &str = r#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new-refresh"},"note":"\ud800"}"#;

#[test]
fn claude_bridge_profile_credentials_migrate_once_and_keep_native_refresh_writeback() {
    let home = tempfile::tempdir().unwrap();
    let original = home.path().join(".claude");
    std::fs::create_dir(&original).unwrap();
    std::fs::write(original.join("work.credentials.json"), OLD).unwrap();
    std::fs::write(original.join("settings.json"), "{}").unwrap();
    let profile = prepare_profile(home.path(), "work").unwrap();
    assert_eq!(std::fs::read_to_string(profile.join(".credentials.json")).unwrap(), OLD);
    assert_eq!(
        std::fs::read_link(profile.join("projects")).unwrap(),
        original.join("projects")
    );
    assert_eq!(std::fs::metadata(&profile).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        std::fs::metadata(profile.join(".credentials.json")).unwrap().mode() & 0o777,
        0o600
    );
    std::fs::write(profile.join(".credentials.json"), NEW).unwrap();
    std::fs::write(original.join("work.credentials.json"), "{broken}").unwrap();
    prepare_profile(home.path(), "work").unwrap();
    assert_eq!(
        read_usage_credentials(home.path(), Some("work"), None).unwrap(),
        js_json::parse_lossless(NEW).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(original.join("work.credentials.json")).unwrap(),
        "{broken}"
    );
}

#[test]
fn claude_bridge_profile_rejects_credential_symlinks_without_disclosing_secrets() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join("secret"), NEW).unwrap();
    symlink(
        home.path().join("secret"),
        home.path().join(".claude/work.credentials.json"),
    )
    .unwrap();
    assert_eq!(
        prepare_profile(home.path(), "work").unwrap_err().message,
        "Credentials must be a regular file, not a symlink"
    );
    assert_eq!(std::fs::read_to_string(home.path().join("secret")).unwrap(), NEW);
    for name in ["../escape", "-bad", "", "\u{e9}"] {
        assert!(validate_names(&[JsString::from(name)]).is_err());
    }
}

#[test]
fn claude_bridge_usage_credentials_errors_match_source() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        read_usage_credentials(home.path(), Some("missing"), None)
            .unwrap_err()
            .message,
        "Missing Claude credentials for current account"
    );
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(home.path().join(".claude/.credentials.json"), "top-secret invalid").unwrap();
    assert_eq!(
        read_usage_credentials(home.path(), None, None).unwrap_err().message,
        "Invalid or unreadable Claude credentials for current account"
    );
}
