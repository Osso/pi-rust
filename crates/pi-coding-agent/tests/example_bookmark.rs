#[path = "../examples/extensions/bookmark.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::{Value, json};

fn assistant(text: &str) -> Value {
    json!({"role":"assistant","content":[{"type":"text","text":text}],"api":"faux","provider":"faux","model":"faux-1",
        "usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},
        "stopReason":"stop","timestamp":0})
}

#[tokio::test]
async fn bookmarks_scan_all_entries_backwards_and_remove_latest_labeled_entry() {
    let actual = harness::compare(
        "bookmark",
        harness::factory(example::bookmark_extension),
        json!({"steps":[
            {"op":"command","name":"bookmark","args":"empty"},
            {"op":"command","name":"unbookmark"},
            {"op":"append","message":assistant("first")},
            {"op":"command","name":"bookmark","args":"old"},
            {"op":"append","message":assistant("latest")},
            {"op":"append","message":{"role":"user","content":"tail","timestamp":0}},
            {"op":"command","name":"bookmark","args":"\u{feff}  🚀 latest \u{2028}"},
            {"op":"label","index":3,"value":"user label"},
            {"op":"command","name":"unbookmark","args":"ignored"},
            {"op":"command","name":"unbookmark"},
            {"op":"command","name":"unbookmark"},
            {"op":"command","name":"unbookmark"}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"],
        json!([
            ["notify", "No assistant message to bookmark", "warning"],
            ["notify", "No bookmarked entry found", "warning"],
            ["setLabel", 0, "old"],
            ["notify", "Bookmarked as: old", "info"],
            ["setLabel", 2, "🚀 latest"],
            ["notify", "Bookmarked as: 🚀 latest", "info"],
            ["setLabel", 3, null],
            ["notify", "Removed bookmark: user label", "info"],
            ["setLabel", 2, null],
            ["notify", "Removed bookmark: 🚀 latest", "info"],
            ["setLabel", 0, null],
            ["notify", "Removed bookmark: old", "info"],
            ["notify", "No bookmarked entry found", "warning"]
        ])
    );
}

#[tokio::test]
async fn default_label_uses_current_milliseconds_and_setter_errors_stop_notification() {
    let before = pi_js_compat::date::js_now_millis();
    let actual = harness::compare(
        "bookmark",
        harness::factory(example::bookmark_extension),
        json!({"steps":[
            {"op":"append","message":assistant("a")},
            {"op":"command","name":"bookmark","args":" \t"}
        ]}),
    )
    .await;
    let label = actual["trace"][0][2].as_str().unwrap();
    let time: f64 = label.strip_prefix("bookmark-").unwrap().parse().unwrap();
    assert!(time >= before && time <= pi_js_compat::date::js_now_millis());
    let error = harness::compare(
        "bookmark",
        harness::factory(example::bookmark_extension),
        json!({"actionError":"label rejected","steps":[
            {"op":"append","message":assistant("a")},
            {"op":"command","name":"bookmark","args":"named"},
            {"op":"label","index":0,"value":"existing"},
            {"op":"command","name":"unbookmark"}
        ]}),
    )
    .await;
    assert_eq!(
        error["trace"],
        json!([["error", "label rejected"], ["error", "label rejected"]])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
