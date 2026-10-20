//! TS behavioral blocker proof only. No native game implementation is shipped.
#[path = "support/oracle.rs"]
mod oracle;
use serde_json::json;
#[test]
fn real_ts_tui_key_release_stops_player_but_held_input_keeps_moving() {
    let mut ts = oracle::TsOracle::spawn(
        "extension-examples-l-invaders-blocker-oracle.ts",
        "packages/coding-agent/examples/extensions/space-invaders.ts",
    );
    for release in [true, false] {
        let mut steps = vec![json!({"op":"key","key":"a"}), json!({"op":"ticks","count":2,"ms":50})];
        if release {
            steps.push(json!({"op":"key","key":"\u{1b}[97;1:3u"}));
        }
        steps.extend([
            json!({"op":"ticks","count":2,"ms":50}),
            json!({"op":"key","key":"\u{1b}"}),
        ]);
        let result = ts.request(&json!({"random":[],"commands":[{"widths":[24,64,90],"steps":steps}]}));
        assert_eq!(result["metadata"], json!([["invaders", "Play Space Invaders!"]]));
        assert_eq!(
            result["entries"][0]["data"]["player"]["x"],
            if release { 29 } else { 28 },
            "TS real TUI must deliver opted-in Kitty release"
        );
        let frames: Vec<_> = result["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v[0] == "render")
            .collect();
        assert!(frames.len() >= 15);
        assert!(
            frames
                .iter()
                .all(|f| f[2].as_array().is_some_and(|lines| lines.len() == 30))
        );
        assert!(frames.iter().any(|f| {
            f[2].as_array()
                .unwrap()
                .iter()
                .any(|line| line.as_str().unwrap().contains("\u{1b}[32m"))
        }));
    }
}
