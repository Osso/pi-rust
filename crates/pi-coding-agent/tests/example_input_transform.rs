#[path = "../examples/extensions/input_transform.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn transforms_queries_handles_ping_and_preserves_unrelated_input_and_images() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::input_transform_extension(api);
        Box::pin(async move { result })
    });
    let image = json!({"type":"image","data":"AA==","mimeType":"image/png"});
    let mut cases = Vec::new();
    for source in ["interactive", "rpc", "extension"] {
        for text in [
            "?quick What is Rust?",
            "?quick \u{feff}\u{a0}λ🚀\u{2028}",
            "?quick   ",
            "?quick",
            "?QUICK hello",
            "ping",
            "PiNg",
            "PING ",
            "Time ",
            "other",
            "?quick \u{85}",
        ] {
            cases.push(json!({"event":"input","text":text,"source":source,"images":[image]}));
        }
    }
    let actual = support::compare("input-transform", factory, json!({"cases":cases})).await;
    assert_eq!(
        actual["cases"][0]["result"],
        json!({"action":"transform","text":"Respond briefly in 1-2 sentences: What is Rust?","images":[image]})
    );
    assert_eq!(
        actual["cases"][2]["ui"],
        json!([["notify", "Usage: ?quick <question>", "warning"]])
    );
    assert_eq!(actual["cases"][5]["ui"], json!([["notify", "pong", "info"]]));
    assert_eq!(actual["cases"][22]["result"], json!({"action":"continue"}));
}

#[tokio::test]
async fn time_input_uses_shared_en_us_formatter_with_an_injected_clock() {
    let millis = 1_767_355_506_000_i64;
    let factory: ExtensionFactory = Arc::new(move |api| {
        let result = example::register_with_clock(api, Arc::new(move || millis));
        Box::pin(async move { result })
    });
    let actual = support::compare(
        "input-transform",
        factory,
        json!({"nowMillis":millis,"cases":[
            {"event":"input","text":"TiMe","source":"interactive"},
            {"event":"input","text":"time","source":"rpc"},
            {"event":"input","text":"time","source":"extension"}
        ]}),
    )
    .await;
    assert_eq!(actual["cases"][0]["result"], json!({"action":"handled"}));
    assert_eq!(
        actual["cases"][0]["ui"][0][1],
        pi_js_compat::date::to_locale_string(millis as f64, pi_js_compat::date::LocaleDateFormat::DateTimeEnUs)
    );
    assert_eq!(actual["cases"][2]["result"], json!({"action":"continue"}));
}

#[test]
fn en_us_date_local_fields_match_pinned_node() {
    if std::env::var_os("PI_DATE_ZONE_CHILD").is_none() {
        for zone in ["UTC", "America/New_York"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "en_us_date_local_fields_match_pinned_node", "--nocapture"])
                .env("PI_DATE_ZONE_CHILD", "1")
                .env("TZ", zone)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "zone {zone}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    // Midnight/noon, one- and two-digit date/hour fields, just before/after both
    // New York DST transitions, and fractional-second truncation.
    let dates = [
        "2026-01-02T00:05:06Z",
        "2026-01-02T12:05:06Z",
        "2026-01-02T01:02:03Z",
        "2026-11-22T23:45:56Z",
        "2026-03-08T06:59:59Z",
        "2026-03-08T07:00:00Z",
        "2026-11-01T05:59:59Z",
        "2026-11-01T06:00:00Z",
        "2026-01-02T00:05:06.999Z",
        "2026-01-02T05:05:06Z",
        "2026-01-02T17:05:06Z",
    ];
    let millis: Vec<_> = dates
        .into_iter()
        .map(|date| pi_js_compat::date::date_parse_millis(date).unwrap())
        .collect();
    let actual: Vec<_> = millis
        .iter()
        .map(|ms| pi_js_compat::date::to_locale_string(*ms as f64, pi_js_compat::date::LocaleDateFormat::DateTimeEnUs))
        .collect();
    let zone = std::env::var_os("TZ").unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "extension-examples-b-date-oracle.ts",
        "packages/coding-agent/examples/extensions/input-transform.ts",
        &[
            ("LANG", "en_US.UTF-8".into()),
            ("LC_ALL", "en_US.UTF-8".into()),
            ("TZ", zone),
        ],
    );
    let expected = oracle.request(&json!({"millis":millis}));
    assert_eq!(expected["locale"], "en-US");
    assert_eq!(json!(actual), expected["dates"]);
    assert!(actual.iter().all(|date| !date.contains('\u{202f}')));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
