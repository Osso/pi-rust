//! Lone UTF-16 units in migrated JSON survive like `JSON.parse`/`JSON.stringify`,
//! and are replaced with U+FFFD only where Node encodes a path for the filesystem.

use std::fs;

use pi_coding_agent::migrations::{MigrationOutput, run_migrations_with};
use pi_js_compat::js_json::JsString;
use tempfile::TempDir;

struct SilentOutput;

impl MigrationOutput for SilentOutput {
    fn line(&self, _text: &JsString) {}
}

fn run(agent: &std::path::Path, project: &std::path::Path) -> Vec<JsString> {
    run_migrations_with(
        agent,
        &agent.join("bin"),
        &JsString::from(project.to_str().unwrap()),
        &SilentOutput,
    )
    .migrated_auth_providers
}

#[test]
fn oauth_and_api_key_providers_keep_lone_surrogates() {
    let temp = TempDir::new().unwrap();
    let agent = temp.path().join("agent");
    let project = temp.path().join("project");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(agent.join("oauth.json"), r#"{"\ud800o":{"refresh":"r\udc00"}}"#).unwrap();
    fs::write(agent.join("settings.json"), r#"{"apiKeys":{"k\udfff":"sk\ud834"}}"#).unwrap();

    let providers = run(&agent, &project);

    assert_eq!(
        providers
            .iter()
            .map(|provider| provider.units().to_vec())
            .collect::<Vec<_>>(),
        [vec![0xd800, 0x6f], vec![0x6b, 0xdfff]]
    );
    let auth = fs::read_to_string(agent.join("auth.json")).unwrap();
    assert_eq!(
        auth,
        "{\n  \"\\ud800o\": {\n    \"type\": \"oauth\",\n    \"refresh\": \"r\\udc00\"\n  },\n  \
         \"k\\udfff\": {\n    \"type\": \"api_key\",\n    \"key\": \"sk\\ud834\"\n  }\n}"
    );
    assert_eq!(fs::read_to_string(agent.join("settings.json")).unwrap(), "{}");
}

#[test]
fn session_header_cwd_with_lone_surrogate_moves_to_replacement_directory() {
    let temp = TempDir::new().unwrap();
    let agent = temp.path().join("agent");
    let project = temp.path().join("project");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(&project).unwrap();
    let header = r#"{"type":"session","cwd":"/w\ud800:x"}"#;
    fs::write(agent.join("s.jsonl"), format!("{header}\n")).unwrap();

    run(&agent, &project);

    let moved = agent.join("sessions").join("--w\u{fffd}-x--").join("s.jsonl");
    assert_eq!(fs::read_to_string(moved).unwrap(), format!("{header}\n"));
    assert!(!agent.join("s.jsonl").exists());
}
