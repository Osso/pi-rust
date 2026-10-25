#[path = "../examples/extensions/with_deps.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn duration_tool_matches_real_ms_package_and_extension() {
    let mut durations = vec![
        "".to_owned(),
        " ".into(),
        "invalid".into(),
        "1.".into(),
        "+1h".into(),
        "1e3".into(),
        " 1h".into(),
        "1h ".into(),
        "1\nh".into(),
        "1\th".into(),
        "1h\n".into(),
        "1h\r".into(),
        "1h\r\n".into(),
        "NaN".into(),
        "Infinity".into(),
        "1秒".into(),
        "１h".into(),
        "-0".into(),
        "-.5h".into(),
        "0.001ms".into(),
        "1".repeat(100),
        "1".repeat(101),
        "🚀".repeat(51),
    ];
    for unit in [
        "",
        "ms",
        "millisecond",
        "milliseconds",
        "msec",
        "msecs",
        "s",
        "sec",
        "secs",
        "second",
        "seconds",
        "m",
        "min",
        "mins",
        "minute",
        "minutes",
        "h",
        "hr",
        "hrs",
        "hour",
        "hours",
        "d",
        "day",
        "days",
        "w",
        "week",
        "weeks",
        "y",
        "yr",
        "yrs",
        "year",
        "years",
    ] {
        for number in ["0", "1", "-2", ".5", "-.25", "1.25", "0002"] {
            for space in ["", " ", "   "] {
                durations.push(format!("{number}{space}{}", unit.to_uppercase()));
            }
        }
    }
    let mut cases = vec![json!({"event":"tool_metadata","toolName":"parse_duration"})];
    cases.extend(
        durations
            .into_iter()
            .map(|duration| json!({"event":"execute","toolName":"parse_duration","params":{"duration":duration}})),
    );
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::with_deps_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare("with-deps/index", factory, json!({"cases":cases})).await;
    assert_eq!(
        actual["cases"][1]["result"]["error"],
        "val is not a non-empty string or a valid number. val=\"\""
    );
    assert_ne!(actual["cases"][2]["result"], Value::Null);
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
