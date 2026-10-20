#[path = "../examples/extensions/border_status_editor.rs"]
mod border_status_editor;
#[path = "../examples/extensions/modal_editor.rs"]
mod modal_editor;
#[path = "../examples/extensions/overlay_test.rs"]
mod overlay_test;
#[path = "../examples/extensions/rainbow_editor.rs"]
mod rainbow_editor;
#[path = "support/extension_examples_editors.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn modal_editor_matches_pinned_input_state_and_ansi() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = modal_editor::modal_editor_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "modal-editor",
        factory,
        json!({"widths":[1,8,20,70],"steps":[
            {"input":"abc"},{"input":"\u{1b}"},{"input":"h"},{"input":"x"},{"input":"q"},
            {"input":"0"},{"input":"a"},{"input":"Z"},{"input":"\u{1b}"},{"input":"$"},
            {"input":"k"},{"input":"j"},{"input":"l"},{"input":"i"},{"input":"\n"},
            {"focus":false},{"focus":true},{"input":"\u{1b}"},{"input":"\u{1b}"},{"input":"\u{3}"}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn rainbow_editor_matches_pinned_animation_and_stopping() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = rainbow_editor::rainbow_editor_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "rainbow-editor",
        factory,
        json!({"widths":[8,20,70],"steps":[
            {"input":"UlTrAtHiNk ultrathink"},{"tick":60},{"tick":60},{"tick":480},{"tick":600},
            {"input":"\u{15}"},{"tick":600},{"input":"ultrathink"},{"tick":60},{"focus":false},{"focus":true}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn border_status_editor_matches_pinned_borders_and_spinner_lifecycle() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = border_status_editor::border_status_editor_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "border-status-editor",
        factory,
        json!({"widths":[1,8,20,70,100],"steps":[
            {"input":"status"},{"event":"agent_start"},{"tick":80},{"tick":720},
            {"event":"agent_end"},{"tick":160},{"event":"agent_start"},{"tick":80},
            {"event":"session_shutdown"},{"tick":160}
        ]}),
    )
    .await;
}

#[tokio::test]
async fn border_status_editor_matches_live_model_context_home_and_no_branch() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = border_status_editor::border_status_editor_extension(api);
        Box::pin(async move { result })
    });
    support::compare("border-status-editor", factory, json!({
        "widths":[2,5,30,120], "repository":false, "homeCwd":true,
        "model":{"provider":"openai","id":"fixture","contextWindow":128000},
        "usage":{"tokens":32767,"contextWindow":128500,"percent":25.5},
        "thinking":"high", "steps":[
            {"thinking":"off","usage":{"tokens":null,"contextWindow":128500,"percent":null}},
            {"model":null,"usage":null},
            {"model":{"provider":"test","id":"updated","contextWindow":64000},"usage":{"tokens":0,"contextWindow":0,"percent":0}},
            {"usage":{"tokens":1000,"contextWindow":64000,"percent":1.5},"thinking":"low"},
            {"event":"session_shutdown"}
        ]
    })).await;
}

#[tokio::test]
async fn overlay_test_matches_pinned_focus_inputs_options_and_result() {
    for steps in [
        json!([
            {"input":"中"},{"input":"x"},{"input":"\u{1b}[D"},{"input":"a"},{"input":"\u{7f}"},
            {"focus":false},{"focus":true},{"input":"\u{1b}[C"},{"input":"\u{1b}[B"},{"input":"r"},
            {"input":"🚀"},{"input":"\u{1b}[B"},{"input":"z"},{"input":"\u{1b}[A"},{"input":"\r"}
        ]),
        json!([{"input":"\u{1b}[A"},{"input":"\u{1b}[B"},{"input":"\u{1b}[B"},{"input":"\u{1b}[B"},{"input":"\u{1b}[B"},{"input":"\r"}]),
        json!([{"input":"\u{1b}"}]),
    ] {
        let factory: ExtensionFactory = Arc::new(|api| {
            let result = overlay_test::overlay_test_extension(api);
            Box::pin(async move { result })
        });
        support::compare("overlay-test", factory, json!({"widths":[20,70,100],"steps":steps})).await;
    }
}
