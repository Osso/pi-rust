#[path = "../examples/extensions/truncated_tool.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;
#[tokio::test]
async fn real_rg_results_truncation_artifacts_errors_and_renderers_match() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("small.txt"), "needle one\nother\nneedle λ🚀\n").unwrap();
    std::fs::write(temp.path().join("large.txt"), "needle data\n".repeat(2200)).unwrap();
    std::fs::write(temp.path().join("bytes.txt"), format!("needle {}\n", "λ".repeat(30000))).unwrap();
    let mut cases = vec![json!({"event":"tool_metadata","toolName":"rg"})];
    for params in [
        json!({"pattern":"needle","path":"small.txt"}),
        json!({"pattern":"absent","path":"small.txt"}),
        json!({"pattern":"needle","path":".","glob":"small.txt"}),
        json!({"pattern":"needle","path":"large.txt"}),
        json!({"pattern":"needle","path":"bytes.txt"}),
        json!({"pattern":"[","path":"small.txt"}),
        json!({"pattern":"needle","path":"nonexistent"}),
    ] {
        cases.push(json!({"event":"execute","toolName":"rg","params":params}));
        cases.push(json!({"event":"render_call","toolName":"rg","params":params,"width":30}));
    }
    for partial in [false, true] {
        for expanded in [false, true] {
            for details in [
                Value::Null,
                json!({"matchCount":0}),
                json!({"matchCount":25.0}),
                json!({"matchCount":25.0,"truncation":{"truncated":true},"fullOutputPath":"/fixture/output.txt"}),
            ] {
                cases.push(json!({"event":"render_result","toolName":"rg","width":32,"isPartial":partial,"expanded":expanded,"result":{"content":[{"type":"text","text":"line\n".repeat(24)}],"details":details}}));
            }
        }
    }
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::truncated_tool_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare(
        "truncated-tool",
        factory,
        json!({"cwd":temp.path(),"captureOutputFiles":true,"cases":cases}),
    )
    .await;
    assert_eq!(
        actual["cases"][3]["result"]["value"],
        json!({"content":[{"type":"text","text":"No matches found"}],"details":{"pattern":"absent","path":"small.txt","matchCount":0}})
    );
}

#[tokio::test]
async fn enforces_pinned_execsync_hundred_megabyte_buffer() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("buffer.txt"),
        format!("needle {}\n", "a".repeat(100 * 1024 * 1024)),
    )
    .unwrap();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::truncated_tool_extension(api);
        Box::pin(async move { result })
    });
    support::compare("truncated-tool",factory,json!({"cwd":temp.path(),"cases":[{"event":"execute","toolName":"rg","params":{"pattern":"needle","path":"buffer.txt"}}]})).await;
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
