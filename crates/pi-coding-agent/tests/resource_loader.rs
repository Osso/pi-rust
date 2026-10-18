use std::fs;

use pi_coding_agent::core::resource_loader::{first_party_extension_name, inline_extension_name, resolve_prompt_input};
use tempfile::TempDir;

#[test]
fn names_first_party_and_inline_extension_paths() {
    assert_eq!(first_party_extension_name("<first-party:browser>"), Some("browser"));
    assert_eq!(first_party_extension_name("<inline>"), None);
    assert_eq!(first_party_extension_name("<first-party:broken"), None);

    assert_eq!(inline_extension_name("<first-party:browser>"), Some("browser"));
    assert_eq!(inline_extension_name("<inline>"), Some("inline"));
    assert_eq!(inline_extension_name("/tmp/extension.ts"), None);
    assert_eq!(inline_extension_name("<>"), Some(""));
}

#[test]
fn resolves_prompt_input_files_and_literals() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("SYSTEM.md");
    fs::write(&path, "System prompt text.\n").unwrap();

    assert_eq!(
        resolve_prompt_input(Some(path.to_str().unwrap()), "system prompt").as_deref(),
        Some("System prompt text.\n")
    );
    assert_eq!(
        resolve_prompt_input(Some("Literal prompt text."), "system prompt").as_deref(),
        Some("Literal prompt text.")
    );
    assert_eq!(resolve_prompt_input(None, "system prompt"), None);
}
