#[path = "../examples/extensions/inline_bash.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn expands_real_private_processes_and_preserves_full_input_results() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("private.txt"), "private-data\n").unwrap();
    let mut cases = Vec::new();
    for source in ["interactive", "rpc", "extension"] {
        for text in [
            "plain",
            "!echo !{printf ignored}",
            "  ! echo !{printf ignored}",
            "!{}",
            "!{printf hello}",
            "prefix !{cat private.txt} / !{printf world}",
            "!{printf duplicate} !{printf duplicate}",
            "!{printf err >&2; exit 7}",
            "!{printf yes; printf no >&2; exit 2}",
            "!{exit 4}",
            "!{printf '$$-$&-$`-$\''}",
            "!{printf 'λ🚀   '}!{printf next}",
            "!{printf '%060d' 0}",
        ] {
            cases.push(json!({"event":"input","text":text,"source":source,"images":[{"type":"image","data":"AA==","mimeType":"image/png"}]}));
        }
    }
    cases.push(json!({"event":"input","text":"!{printf silent}","source":"rpc","hasUI":false}));
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::inline_bash_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare("inline-bash", factory, json!({"cwd":temp.path(),"cases":cases})).await;
    assert_eq!(actual["cases"][5]["result"]["text"], "prefix private-data / world");
    assert_eq!(
        actual["cases"][7]["ui"][0][1],
        "Expanded 1 inline command(s):\n!{printf err >&2; exit 7} (exit code 7) -> \"err\""
    );
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
