use std::fs;

use pi_coding_agent::core::pi_manifest::{PiManifest, read_pi_manifest};

fn manifest_from(contents: &[u8]) -> Option<PiManifest> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("package.json");
    fs::write(&path, contents).unwrap();
    read_pi_manifest(&path)
}

#[test]
fn keeps_only_string_array_resource_fields() {
    let manifest = manifest_from(
        br#"{"pi":{"extensions":["a.ts","b.ts"],"skills":["s",1],"prompts":"p","themes":[],"other":["x"]}}"#,
    )
    .unwrap();
    assert_eq!(
        manifest,
        PiManifest {
            extensions: Some(vec!["a.ts".into(), "b.ts".into()]),
            skills: None,
            prompts: None,
            themes: Some(Vec::new()),
        }
    );
    assert_eq!(
        serde_json::to_string(&manifest).unwrap(),
        r#"{"extensions":["a.ts","b.ts"],"themes":[]}"#
    );
}

#[test]
fn returns_an_empty_manifest_for_an_empty_pi_object() {
    assert_eq!(manifest_from(br#"{"pi":{}}"#), Some(PiManifest::default()));
}

#[test]
fn returns_none_without_an_object_pi_field() {
    assert_eq!(manifest_from(br#"{"name":"pkg"}"#), None);
    assert_eq!(manifest_from(br#"{"pi":["a"]}"#), None);
    assert_eq!(manifest_from(br#"{"pi":null}"#), None);
    assert_eq!(manifest_from(br#"["pi"]"#), None);
}

#[test]
fn returns_none_for_unreadable_or_invalid_json() {
    assert_eq!(manifest_from(b"{not json"), None);
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(read_pi_manifest(&directory.path().join("missing.json")), None);
}
