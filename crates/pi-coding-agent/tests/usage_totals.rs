use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, TextOrImageContent, ToolResultMessage,
    ToolResultRole, Usage, UsageCost,
};
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionMessageEntry};
use pi_coding_agent::core::usage_totals::{
    UsageTotals, add_usage_to_totals, create_usage_totals, get_usage_cost_breakdown,
};
use pi_js_compat::js_json::JsString;

fn message_entry(message: AgentMessage) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: JsString::from("x"),
        parent_id: None,
        timestamp: JsString::default(),
        message: Box::new(message),
    }))
}

fn assistant(provider: &str, model: &str, usage: Usage) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(""))],
        api: "anthropic-messages".to_owned(),
        provider: provider.to_owned(),
        model: model.to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage,
        stop_reason: StopReason::Stop,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: 0,
    }))
}

fn usage(input: f64, output: f64, cost: f64) -> Usage {
    Usage {
        input,
        output,
        total_tokens: input + output,
        cost: UsageCost {
            total: cost,
            ..UsageCost::default()
        },
        ..Usage::default()
    }
}

#[test]
fn creates_and_accumulates_usage_totals() {
    let mut totals = create_usage_totals();
    assert_eq!(totals, UsageTotals::default());
    add_usage_to_totals(
        &mut totals,
        &Usage {
            input: 1.0,
            output: 2.0,
            cache_read: 3.0,
            cache_write: 4.0,
            cost: UsageCost {
                total: 5.0,
                ..UsageCost::default()
            },
            ..Usage::default()
        },
    );
    assert_eq!(totals.input, 1.0);
    assert_eq!(totals.output, 2.0);
    assert_eq!(totals.cache_read, 3.0);
    assert_eq!(totals.cache_write, 4.0);
    assert_eq!(totals.cost, 5.0);
}

#[test]
fn groups_usage_by_model_and_tool_bucket() {
    let entries = vec![
        message_entry(assistant("anthropic", "claude-test", usage(10.0, 20.0, 1.5))),
        message_entry(assistant("anthropic", "claude-test", usage(5.0, 5.0, 0.5))),
        message_entry(assistant("openai", "gpt-test", usage(100.0, 0.0, 0.1))),
        message_entry(AgentMessage::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "call".to_owned(),
            tool_name: "tool".to_owned(),
            content: vec![TextOrImageContent::Text(TextContent::new("result"))],
            details: None,
            usage: Some(usage(7.0, 3.0, 0.25)),
            added_tool_names: None,
            is_error: false,
            timestamp: 0,
        })),
        message_entry(assistant("anthropic", "free", usage(0.0, 0.0, 0.0))),
    ];
    let breakdown = get_usage_cost_breakdown(&entries);
    assert_eq!(breakdown.len(), 3);
    assert_eq!(breakdown[0].key, "anthropic/claude-test");
    assert_eq!(breakdown[0].cost, 2.0);
    assert_eq!(breakdown[0].tokens, 40.0);
    assert_eq!(breakdown[1].key, "Tools/summaries");
    assert_eq!(breakdown[1].cost, 0.25);
    assert_eq!(breakdown[1].tokens, 10.0);
    assert_eq!(breakdown[2].key, "openai/gpt-test");
    assert_eq!(breakdown[2].cost, 0.1);
    assert_eq!(breakdown[2].tokens, 100.0);
}

#[test]
fn uses_response_model_when_present() {
    let mut message = assistant("anthropic", "requested", usage(1.0, 1.0, 0.1));
    if let AgentMessage::Assistant(assistant) = &mut message {
        assistant.response_model = Some("actual".to_owned());
    }
    let breakdown = get_usage_cost_breakdown(&[message_entry(message)]);
    assert_eq!(breakdown[0].key, "anthropic/actual");
}
