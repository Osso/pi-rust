use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

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
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
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

struct AnswerUi(JsValue);
impl ExtensionUiContext for AnswerUi {
    fn custom<'a>(
        &'a self,
        _: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let answer = self.0.clone();
        Box::pin(async move { Ok(Some(answer)) })
    }
}

#[tokio::test]
async fn validation_headless_and_raw_answer_metadata_match_frozen() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let question = JsString::from("Question-") + &raw;
    let answer = json!({"answers":{},"cancelled":false});
    let mut raw_answers = pi_js_compat::js_json::JsObject::new();
    raw_answers.insert(question.clone(), JsValue::String(JsString::from("answer-") + &raw));
    let mut oracle = oracle::TsOracle::spawn("coding-agent-session-tools-oracle.ts", "packages/coding-agent/src");
    let cases = [
        (
            false,
            ExtensionMode::Print,
            json!({"blocking":true,"questions":[],"metadata":{"raw":raw}}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Rpc,
            json!({"blocking":true,"questions":[]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":"1"},{"question":"2"},{"question":"3"},{"question":"4"},{"question":"5"}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question,"multiSelect":true}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question,"options":[{"label":"A"}]}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question,"options":[{"label":"1"},{"label":"2"},{"label":"3"},{"label":"4"},{"label":"5"}]}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question},{"question":question}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question,"options":[{"label":raw},{"label":raw}]}]}),
            answer.clone(),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":question,"header":raw,"multiSelect":false}],"metadata":{"raw":raw,"nested":[null,1,true]}}),
            json!({"answers":JsValue::Object(raw_answers),"cancelled":false}),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":"","options":[{"label":"","preview":raw},{"label":"B","description":raw}]}]}),
            json!({"answers":{"":""},"cancelled":false}),
        ),
        (
            true,
            ExtensionMode::Tui,
            json!({"blocking":true,"questions":[{"question":"10"},{"question":"2"}]}),
            json!({"answers":{"10":raw,"2":""},"cancelled":false}),
        ),
    ];
    for (has_ui, mode, params, answer) in cases {
        let mut request = json!({"id":1,"operation":"tool:execute","module":"core/tools/ask-questions.ts","factory":"createAskQuestionsToolDefinition","params":params,"cwd":"/repo"});
        let mut builder =
            tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
                .mode(mode);
        if has_ui {
            builder = builder.ui(Arc::new(AnswerUi(answer.clone())));
            request["uiScript"] = json!({"panelResult":answer});
            request["contextMode"] = json!(if mode == ExtensionMode::Rpc { "rpc" } else { "tui" });
        }
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let context = builder.build().await.1;
        let outcome = AskQuestionsTool::new()
            .execute("tool-call".into(), params.clone(), None, None, Some(context), None)
            .await;
        let actual = match outcome {
            Ok(result) => {
                json!({"ok":true,"content":result.content,"details":result.details,"isError":result.is_error,"terminate":result.terminate})
            }
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        assert_eq!(actual, expected["output"], "{params:?}");
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
