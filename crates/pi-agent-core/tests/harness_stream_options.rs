//! Behavioral port of the harness orchestration value types in `packages/agent/src/harness/types.ts`.

use indexmap::IndexMap;
use std::sync::Arc;

use pi_agent_core::agent::{AgentToolResult, ToolError};
use pi_agent_core::harness::types::{
    AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, AgentHarnessTool, AgentHarnessToolContextSource,
    OptionalPatch,
};
use pi_ai::request::BoxFuture;
use pi_ai::types::{CacheRetention, Transport};
use serde_json::{Value, json};

#[test]
fn stream_options_keep_absent_and_explicit_values_distinct() {
    let options = AgentHarnessStreamOptions::default();
    assert_eq!(options.transport, None);
    assert_eq!(options.timeout_ms, None);
    assert_eq!(options.websocket_connect_timeout_ms, None);
    assert_eq!(options.max_retries, None);
    assert_eq!(options.max_retry_delay_ms, None);
    assert!(options.headers.is_none());
    assert!(options.metadata.is_none());
    assert_eq!(options.cache_retention, None);
    assert!(options.on_retry.is_none());

    let options = AgentHarnessStreamOptions {
        transport: Some(Transport::WebsocketCached),
        timeout_ms: Some(30_000.0),
        websocket_connect_timeout_ms: Some(5_000.0),
        max_retries: Some(2.0),
        max_retry_delay_ms: Some(1_000.0),
        headers: Some(
            [
                ("zeta".to_owned(), "1".to_owned()),
                ("alpha".to_owned(), "2".to_owned()),
            ]
            .into_iter()
            .collect(),
        ),
        metadata: Some(json!({"trace": "abc"}).as_object().unwrap().clone()),
        cache_retention: Some(CacheRetention::Long),
        on_retry: None,
    };
    assert_eq!(options.transport, Some(Transport::WebsocketCached));
    assert_eq!(options.timeout_ms, Some(30_000.0));
    assert_eq!(options.max_retries, Some(2.0));
    assert_eq!(options.headers.as_ref().unwrap()["zeta"], "1");
    let header_keys: Vec<&str> = options.headers.as_ref().unwrap().keys().map(String::as_str).collect();
    assert_eq!(header_keys, vec!["zeta", "alpha"]);
    assert_eq!(options.metadata.as_ref().unwrap()["trace"], "abc");
    assert_eq!(options.cache_retention, Some(CacheRetention::Long));
}

#[test]
fn stream_option_patch_distinguishes_unchanged_clear_and_set() {
    let patch = AgentHarnessStreamOptionsPatch::default();
    assert_eq!(patch.headers, OptionalPatch::Unchanged);
    assert_eq!(patch.metadata, OptionalPatch::Unchanged);

    let clear = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Clear,
        metadata: OptionalPatch::Clear,
        ..Default::default()
    };
    assert_eq!(clear.headers, OptionalPatch::Clear);
    assert_eq!(clear.metadata, OptionalPatch::Clear);

    let mut headers = IndexMap::new();
    headers.insert("keep".to_owned(), Some("value".to_owned()));
    headers.insert("delete".to_owned(), None);
    let mut metadata = IndexMap::new();
    metadata.insert("keep".to_owned(), Some(json!(1)));
    metadata.insert("delete".to_owned(), None);
    let set = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        metadata: OptionalPatch::Set(metadata.into_iter().collect()),
        timeout_ms: Some(100.0),
        ..Default::default()
    };
    let OptionalPatch::Set(headers) = &set.headers else {
        panic!("headers must be set");
    };
    assert_eq!(headers["delete"], None);
    assert_eq!(set.timeout_ms, Some(100.0));
}

struct EchoTool;

impl AgentHarnessTool<Value> for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo the context"
    }

    fn parameters(&self) -> &Value {
        static PARAMETERS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> &str {
        "Echo"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: Option<pi_agent_core::agent::AgentToolUpdateCallback>,
        context: Value,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                    context["label"].as_str().unwrap_or_default(),
                ))],
                details: None,
                usage: None,
                added_tool_names: None,
                is_error: None,
                terminate: None,
            })
        })
    }
}

#[tokio::test]
async fn harness_tool_executes_with_the_resolved_context() {
    let tool = EchoTool;
    assert_eq!(tool.name(), "echo");
    assert_eq!(tool.label(), "Echo");
    assert_eq!(tool.parameters()["type"], "object");
    let result = tool
        .execute(
            "call-1".to_owned(),
            json!({}),
            None,
            None,
            json!({"label": "turn-context"}),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"content": [{"type": "text", "text": "turn-context"}]})
    );
}

#[tokio::test]
async fn tool_context_source_resolves_static_and_provider_values() {
    let static_source = AgentHarnessToolContextSource::Static(json!({"kind": "static"}));
    let AgentHarnessToolContextSource::Static(value) = static_source else {
        panic!("expected static context");
    };
    assert_eq!(value, json!({"kind": "static"}));

    let provider: Arc<dyn Fn() -> BoxFuture<'static, Value> + Send + Sync> =
        Arc::new(|| Box::pin(async { json!({"kind": "provider"}) }));
    let provider_source = AgentHarnessToolContextSource::Provider(provider);
    let AgentHarnessToolContextSource::Provider(provider) = provider_source else {
        panic!("expected provider context");
    };
    assert_eq!(provider().await, json!({"kind": "provider"}));
}

#[test]
fn patch_headers_use_an_ordered_map() {
    let mut headers = IndexMap::new();
    headers.insert("b".to_owned(), None);
    headers.insert("a".to_owned(), Some("1".to_owned()));
    let patch = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        ..Default::default()
    };
    assert_eq!(patch.headers, OptionalPatch::Set(headers));
}
