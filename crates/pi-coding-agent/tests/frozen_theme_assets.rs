use std::{fs, path::Path};

#[test]
fn bundled_theme_assets_match_frozen_bytes() {
    let lock: serde_json::Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    let frozen =
        Path::new(lock["worktree"].as_str().unwrap()).join("packages/coding-agent/src/modes/interactive/theme");
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("theme");
    for name in ["dark.json", "light.json", "theme-schema.json"] {
        assert_eq!(
            fs::read(bundled.join(name)).unwrap(),
            fs::read(frozen.join(name)).unwrap(),
            "{name}"
        );
    }
}
