#[path = "../examples/extensions/tools.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn tool_selection_ui_persists_order_and_restores_current_branch() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::tools_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare(
        "tools",
        factory,
        json!({"allTools":["read","bash","write"],"activeTools":["bash","read"],"cases":[
            {"event":"session_start"},
            {"event":"command","name":"tools","mode":"rpc"},
            {"event":"command","name":"tools","keys":[" ","\u{1b}[B","\r","\u{1b}[A"," ","\u{1b}"]},
            {"event":"append_custom","kind":"tools-config","data":{"enabledTools":["write","unknown","bash","write"]}},
            {"event":"event","value":{"type":"session_tree","newLeafId":null,"oldLeafId":null}},
            {"event":"command","name":"tools","keys":["\u{1b}[A","\r","\u{3}"],"width":24},
            {"event":"branch","index":3},
            {"event":"session_start"},
            {"event":"command","name":"tools","keys":["\u{1b}"]}
        ]}),
    )
    .await;
    assert_eq!(actual["effects"][0], json!(["setActiveTools", ["bash"]]));
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::tools_extension(api);
        Box::pin(async move { result })
    });
    support::compare("tools",factory,json!({"allTools":[],"activeTools":[],"cases":[{"event":"session_start"},{"event":"command","name":"tools","keys":["\r","\u{1b}[B","\u{1b}"]}]})).await;
    selector_retains_native_cursor_with_remapped_navigation().await;
}

async fn selector_retains_native_cursor_with_remapped_navigation() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::tools_extension(api);
        Box::pin(async move { result })
    });
    support::compare("tools",factory,json!({"allTools":["read","bash","write"],"activeTools":["read"],"keybindings":{"tui.select.down":["j"],"tui.select.up":["k"]},"cases":[{"event":"session_start"},{"event":"command","name":"tools","keys":["j","j"," ","k","\r","k","\u{1b}"]}]})).await;
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
