#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::{
    ContextTool, ExtensionMode, ExtensionUiContext, ExtensionUiCustomFactory, ExtensionUiCustomOptions,
};
use pi_coding_agent::core::tools::ask_questions::AskQuestionsTool;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::Arc;

struct PendingUi;
impl ExtensionUiContext for PendingUi {
    fn custom<'a>(
        &'a self,
        _: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, JsString>> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn explicit_blocking_validation_precedes_missing_context() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src");
    for params in [
        json!({"questions":[]}),
        json!({"blocking":"true","questions":[]}),
        json!({"blocking":true,"questions":[]}),
    ] {
        let response = oracle.request(&json!({"id":1,"operation":"tool:execute","module":"core/tools/ask-questions.ts","factory":"createAskQuestionsToolDefinition","params":params,"withoutContext":true}));
        assert_eq!(response["ok"], json!(true), "{response:?}");
        let error = AskQuestionsTool::new()
            .execute("tool-call".into(), params, None, None, None, None)
            .await
            .unwrap_err();
        assert_eq!(json!({"ok":false,"error":error.message}), response["output"]);
    }
}

#[tokio::test]
async fn empty_control_path_is_rejected_before_starting_a_nonblocking_panel() {
    let root = tempfile::tempdir().unwrap();
    let directory = pi_js_compat::node_fs::decode_node_path(root.path().as_os_str());
    let manager = tool_context::persisted_session("/repo", directory.clone(), "question-session");
    let path = manager.lock().unwrap().get_session_file().unwrap().to_owned();
    let context = tool_context::TestContextBuilder::new("/repo", manager)
        .mode(ExtensionMode::Tui)
        .ui(Arc::new(PendingUi))
        .control_db_path("")
        .build()
        .await
        .1;
    let params = json!({"blocking":false,"questions":[{"question":"Question?"}]});
    let mut oracle = oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src");
    let response=oracle.request(&json!({"id":1,"operation":"tool:execute","module":"core/tools/ask-questions.ts","factory":"createAskQuestionsToolDefinition","params":params,"cwd":"/repo","sessionDir":directory,"sessionFile":path,"controlDbPath":"","uiScript":{"panelResult":{"answers":{},"cancelled":false}}}));
    assert_eq!(response["ok"], json!(true), "{response:?}");
    assert_eq!(
        response["output"],
        json!({"ok":false,"error":"Nonblocking ask_questions requires a persisted session and control database"})
    );
    let outcome = AskQuestionsTool::new()
        .execute("tool-call".into(), params, None, None, Some(context), None)
        .await;
    let actual = match outcome {
        Ok(_) => json!({"ok":true}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    assert_eq!(actual, response["output"]);
}
