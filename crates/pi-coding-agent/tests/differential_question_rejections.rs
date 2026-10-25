#[path = "../../pi-agent-core/tests/support/mod.rs"]
pub mod agent_support;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

use pi_coding_agent::core::extensions::types::{
    BoxFuture, ContextTool, ExtensionContext, ExtensionMode, ExtensionUiContext, ExtensionUiCustomFactory,
    ExtensionUiCustomOptions, ToolDefinition,
};
use pi_coding_agent::core::tools::{
    ask_questions::{AskQuestionsTool, create_ask_questions_tool_definition},
    tool_definition_wrapper::wrap_definition,
};
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json as json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::Arc;

struct RejectingCustom(CallbackError);
impl ExtensionUiContext for RejectingCustom {
    fn custom<'a>(
        &'a self,
        _: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, CallbackError>> {
        let reason = self.0.clone();
        Box::pin(async move { Err(reason) })
    }
}
fn snapshot(error: CallbackError) -> JsValue {
    let value = error.thrown_value();
    let kind = if matches!(value, AbortReason::Error(_) | AbortReason::DomException(_)) {
        "error"
    } else {
        "value"
    };
    json!({"kind":kind,"value":value.value()})
}
fn fixture_rejection(input: &JsValue) -> CallbackError {
    let reason = if input["kind"] == json!("error") {
        AbortReason::Error(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, input["value"].as_string().unwrap().clone())
                .with_property("code", json!("E_CUSTOM")),
        )
    } else {
        AbortReason::Value(input["value"].clone())
    };
    CallbackError::from_rejection(reason)
}
async fn runtime_result(definition: ToolDefinition, context: Option<ExtensionContext>, params: JsValue) -> JsValue {
    let factory = context.map(|context| {
        Arc::new(move || context.clone()) as pi_coding_agent::core::tools::tool_definition_wrapper::ToolContextFactory
    });
    let tool = wrap_definition(definition, factory);
    let stream = pi_agent_core::agent::agent_loop(
        vec![agent_support::user("Ask").into()],
        pi_agent_core::agent::AgentContext {
            system_prompt: JsString::default(),
            messages: Vec::new(),
            tools: Some(vec![tool]),
        },
        pi_agent_core::agent::AgentLoopConfig::new(agent_support::model(), agent_support::identity_convert()),
        None,
        Some(agent_support::scripted_stream_fn(vec![
            agent_support::Script::message(agent_support::tool_use(vec![pi_ai::types::AssistantContent::ToolCall(
                agent_support::tool_call("call", "ask_questions", params),
            )])),
            agent_support::Script::message(agent_support::text_assistant("")),
        ])),
    );
    let (_, messages) = agent_support::collect_events(stream).await;
    let runtime = messages
        .unwrap()
        .into_iter()
        .filter_map(|message| match message {
            pi_agent_core::agent::AgentMessage::ToolResult(message) => Some(message.content),
            _ => None,
        })
        .collect::<Vec<_>>();
    json!(runtime)
}
async fn native_history(input: &JsValue) -> JsValue {
    let ui = Arc::new(RejectingCustom(fixture_rejection(input)));
    let context = tool_context::TestContextBuilder::new("/repo", extension_support::session_manager("/repo"))
        .mode(ExtensionMode::Tui)
        .ui(ui)
        .build()
        .await
        .1;
    let context = (input["missingContext"] != json!(true)).then_some(context);
    let params = json!({"blocking":true,"questions":[{"question":"Question"}]});
    let definition = create_ask_questions_tool_definition();
    let error = (definition.execute)("call".into(), params.clone(), None, None, context.clone())
        .await
        .unwrap_err();
    let direct = snapshot(error.into());
    let adapter = AskQuestionsTool::new()
        .execute("call".into(), params.clone(), None, None, context.clone(), None)
        .await
        .unwrap_err();
    let runtime = runtime_result(definition, context, params).await;
    json!({"direct":direct,"runtime":runtime,"adapterText":adapter.message})
}
#[tokio::test]
async fn custom_rejection_preserves_the_exported_factory_throw_and_agent_loop_result() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let cases = json!([{"kind":"value","value":raw},{"kind":"error","value":raw},{"kind":"value","value":null},{"kind":"value","value":false},{"kind":"value","value":42},{"kind":"value","value":{"message":raw}},{"kind":"value","value":raw,"missingContext":true}]);
    let mut oracle = oracle::TsOracle::spawn(
        "question-custom-rejection-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    for (index, input) in cases.as_array().unwrap().iter().enumerate() {
        let actual = native_history(input).await;
        let expected = &expected["output"][index];
        assert_eq!(actual["direct"], expected["direct"]);
        assert_eq!(actual["runtime"], expected["runtime"]);
        assert_eq!(actual["adapterText"], expected["runtime"][0][0]["text"]);
    }
}
#[tokio::test]
async fn conversion_failure_is_caught_by_frozen_prompt_and_native_reports_the_approved_gap_explicitly() {
    let input = json!({"kind":"value","value":{"toString":null}});
    let mut oracle = oracle::TsOracle::spawn(
        "question-custom-rejection-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
    );
    let expected = oracle.request(&json!({"id":1,"coercionProbe":true,"cases":[input]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let expected = &expected["output"][0];
    assert_eq!(
        expected["prompt"],
        json!({"resolved":true,"isStreaming":false,"errors":["Cannot convert object to primitive value"],"toolResults":0})
    );
    let actual = native_history(&input).await;
    assert_eq!(actual["direct"], expected["direct"]);
    assert_eq!(actual["adapterText"], expected["prompt"]["errors"][0]);
    // Approved 2026-10-24 ingress (c) gap: ToolError cannot carry loop-fatal control flow.
    assert_eq!(actual["runtime"].as_array().unwrap().len(), 1);
    assert_eq!(actual["runtime"][0][0]["text"], expected["prompt"]["errors"][0]);
}
