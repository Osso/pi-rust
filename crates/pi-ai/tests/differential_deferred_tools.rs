#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::Context;
use pi_ai::utils::deferred_tools::{SplitDeferredTools, split_deferred_tools, split_deferred_tools_with};
use serde_json::{Value, json};

fn tool(name: &str, description: &str) -> Value {
    json!({"name": name, "description": description, "parameters": {"type": "object"}})
}

fn requests() -> Vec<Value> {
    let base_context = |tools: Vec<Value>, added: Value| {
        json!({
            "messages": [
                {"role": "user", "content": "hello", "timestamp": 1},
                {
                    "role": "assistant",
                    "content": [{"type": "toolCall", "id": "call_1", "name": "base_tool", "arguments": {}}],
                    "api": "anthropic-messages", "provider": "anthropic", "model": "test",
                    "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                              "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
                    "stopReason": "toolUse", "timestamp": 2
                },
                {
                    "role": "toolResult", "toolCallId": "call_1", "toolName": "base_tool",
                    "content": [{"type": "text", "text": "done"}], "isError": false, "timestamp": 3,
                    "addedToolNames": added
                },
                {"role": "user", "content": "tail", "timestamp": 4}
            ],
            "tools": tools
        })
    };
    vec![
        json!({"id": 0, "enabled": false, "normalize": "identity",
               "context": base_context(vec![tool("base_tool", "base"), tool("late_tool", "late")], json!(["late_tool"]))}),
        json!({"id": 1, "enabled": true, "normalize": "identity",
               "context": base_context(vec![tool("base_tool", "base"), tool("late_tool", "late")], json!(["late_tool"]))}),
        json!({"id": 2, "enabled": true, "normalize": "identity",
               "context": base_context(vec![tool("base_tool", "base"), tool("late_tool", "late")], json!(["base_tool", "late_tool"]))}),
        json!({"id": 3, "enabled": true, "normalize": "lower",
               "context": base_context(vec![tool("BASE_TOOL", "base"), tool("LATE_TOOL", "late")], json!(["Late_Tool"]))}),
        json!({"id": 4, "enabled": false, "normalize": "identity",
               "context": {"messages": [], "tools": [tool("a", "first"), tool("b", "b"), tool("a", "second")]}}),
        json!({"id": 5, "enabled": true, "normalize": "identity", "context": {"messages": []}}),
    ]
}

fn observe(request: &Value) -> Value {
    let context: Context = serde_json::from_value(request["context"].clone()).expect("context");
    let enabled = request["enabled"].as_bool().expect("enabled");
    let split: SplitDeferredTools = if request["normalize"] == "lower" {
        split_deferred_tools_with(&context, enabled, str::to_lowercase)
    } else {
        split_deferred_tools(&context, enabled)
    };
    json!({
        "id": request["id"],
        "ok": true,
        "output": {"immediate": split.immediate, "deferred": split.deferred}
    })
}

#[test]
fn deferred_tool_splitting_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("deferred-tools-oracle.ts", "packages/ai/src/utils/deferred-tools.ts");
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}
