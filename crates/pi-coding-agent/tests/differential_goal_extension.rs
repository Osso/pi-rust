//! Differential coverage for the deterministic goal extension modules.
//!
//! The oracle imports the pinned TypeScript modules and returns their raw
//! values; the Rust side must match byte-for-byte after JSON serialization.

use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;

use pi_coding_agent::bundled_extensions::goal::goal_args::{ParseGoalArgsResult, ParsedGoalArgs, parse_goal_args};
use pi_coding_agent::bundled_extensions::goal::goal_objective::deduplicate_current_goal_scope;
use pi_coding_agent::bundled_extensions::goal::goal_parsing::{parse_goal, parse_goal_json};
use pi_coding_agent::bundled_extensions::goal::goal_presentation::{
    goal_footer_status, goal_startup_message, goal_system_block, goal_view_message,
};
use pi_coding_agent::bundled_extensions::goal::goal_turn::{
    did_last_assistant_abort, did_last_assistant_return_empty, find_last_assistant_message,
};
use pi_coding_agent::core::extensions::types::AgentEndEvent;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsObject as Map;
use pi_js_compat::js_json::JsValue as Value;

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "coding-agent-goal-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-args.ts",
    )
}

fn parsed_args_value(result: &ParseGoalArgsResult) -> Value {
    match result {
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::View) => json!({"action": "view"}),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Pause) => json!({"action": "pause"}),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Resume) => json!({"action": "resume"}),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Clear) => json!({"action": "clear"}),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Set { objective }) => {
            json!({"action": "set", "objective": objective})
        }
        ParseGoalArgsResult::Error(error) => json!({"error": error}),
    }
}

fn goal_value(goal: &pi_coding_agent::bundled_extensions::goal::goal_types::Goal) -> Value {
    pi_js_compat::js_json::to_value(goal).unwrap()
}

fn assistant(text: &str, stop_reason: &str, with_tool_call: bool) -> Value {
    let mut content = vec![json!({"type": "text", "text": text})];
    if with_tool_call {
        content.push(json!({
            "type": "toolCall",
            "id": "call-1",
            "name": "read",
            "arguments": {}
        }));
    }
    json!({
        "role": "assistant",
        "content": content,
        "api": "openai-completions",
        "provider": "test",
        "model": "test-model",
        "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": stop_reason,
        "timestamp": 1
    })
}

#[test]
fn matches_typescript_goal_parsing_args_presentation_and_turn_classification() {
    let mut oracle = oracle();

    let args_cases = [
        "",
        "   ",
        "set ship the thing",
        "set",
        "pause",
        "resume",
        "clear",
        "pause now",
        "ship it",
        "--token-budget 10",
        "--token-budget=10",
        "--wall-clock-minutes 5",
        "--wall-clock-minutes=5",
        "--replace",
        "set  spaced   objective  ",
    ];
    let output = oracle.request(&json!({
        "id": 1,
        "operation": "parse-args",
        "cases": args_cases
    }));
    let expected = output["output"].clone();
    let actual: Vec<Value> = args_cases
        .iter()
        .map(|args| parsed_args_value(&parse_goal_args(*args)))
        .collect();
    assert_eq!(Value::Array(actual), expected);

    let parse_cases = [
        json!({"objective": "  ship it  ", "branch": "main", "createdAt": "2026-01-01T00:00:00.000Z"}),
        json!({
            "objective": "ship it",
            "branch": "main",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "completedAt": "2026-01-02T00:00:00.000Z",
            "completionReason": "done",
            "continuationTurns": 2,
            "pausedAt": "2026-01-03T00:00:00.000Z",
            "pauseReason": "  blocked  ",
            "reviewEvidence": [
                {"kind": "user", "text": "hello"},
                {"kind": "end_turn", "reason": "  "},
                {"kind": "end_turn", "reason": "done"},
                {"kind": "other", "reason": "ignored"}
            ]
        }),
        json!({"branch": "main", "createdAt": "x"}),
        json!({"objective": "", "branch": "main", "createdAt": "x"}),
        json!({"objective": "x", "branch": "", "createdAt": "x"}),
        json!({"objective": "x", "branch": "main"}),
        json!(["x"]),
        json!(null),
    ];
    let output = oracle.request(&json!({
        "id": 2,
        "operation": "parse-goal",
        "cases": parse_cases.iter().map(|value| json!({"kind": "parse-goal", "value": value})).collect::<Vec<_>>()
    }));
    let actual: Vec<Value> = parse_cases
        .iter()
        .map(|value| pi_js_compat::js_json::to_value(&parse_goal(value)).unwrap())
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);

    let json_cases = [
        "not json",
        "{\"objective\":\"x\",\"branch\":\"main\",\"createdAt\":\"now\"}",
        "{}",
    ];
    let output = oracle.request(&json!({
        "id": 3,
        "operation": "parse-goal-json",
        "cases": json_cases.iter().map(|value| json!({"kind": "parse-goal-json", "value": value})).collect::<Vec<_>>()
    }));
    let actual: Vec<Value> = json_cases
        .iter()
        .map(|value| pi_js_compat::js_json::to_value(&parse_goal_json(&JsString::from(*value))).unwrap())
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);

    let dedupe_cases = [
        ("Ship A; Ship B", "Ship A"),
        ("Ship A; Ship A; Ship B", "Ship A"),
        ("Ship A; Ship A", "Ship A"),
        ("Ship A", "Ship A"),
        ("other", "Ship A"),
        ("  Ship A; Ship B  ", "Ship A"),
    ];
    let output = oracle.request(&json!({
        "id": 4,
        "operation": "dedupe",
        "cases": dedupe_cases.iter().map(|(reviewed, current)| json!({"kind": "dedupe", "reviewed": reviewed, "current": current})).collect::<Vec<_>>()
    }));
    let actual: Vec<Value> = dedupe_cases
        .iter()
        .map(|(reviewed, current)| {
            Value::String(deduplicate_current_goal_scope(
                &JsString::from(*reviewed),
                &JsString::from(*current),
            ))
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);

    let goals = [
        json!({
            "objective": "ship it",
            "branch": "main",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "continuationTurns": 3
        }),
        json!({
            "objective": "ship it",
            "branch": "main",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "pausedAt": "2026-01-02T00:00:00.000Z"
        }),
        json!({
            "objective": "ship it",
            "branch": "main",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "pausedAt": "2026-01-02T00:00:00.000Z",
            "pauseReason": "  blocked  "
        }),
    ];
    let output = oracle.request(&json!({
        "id": 5,
        "operation": "present",
        "cases": goals.iter().map(|goal| json!({"kind": "present", "goal": goal})).collect::<Vec<_>>()
    }));
    let actual: Vec<Value> = goals
        .iter()
        .map(|value| {
            let goal = parse_goal(value).unwrap();
            json!({
                "footer": goal_footer_status(&goal),
                "startup": goal_startup_message(&goal),
                "view": goal_view_message(&goal),
                "block": goal_system_block(&goal)
            })
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);

    let events = [
        json!({"messages": [assistant("still working", "stop", false)], "sessionContinuation": null}),
        json!({"messages": [assistant("", "aborted", false)]}),
        json!({"messages": [assistant("   ", "stop", false)]}),
        json!({"messages": [assistant("", "stop", true)]}),
        json!({"messages": [assistant("", "error", false)]}),
        json!({"messages": [
            assistant("first", "stop", false),
            {"role": "user", "content": "hi", "timestamp": 2},
            assistant("last", "toolUse", true)
        ]}),
        json!({"messages": []}),
    ];
    let output = oracle.request(&json!({
        "id": 6,
        "operation": "turn",
        "cases": events.iter().map(|event| json!({"kind": "turn", "event": event})).collect::<Vec<_>>()
    }));
    let actual: Vec<Value> = events
        .iter()
        .map(|value| {
            let event: AgentEndEvent = pi_js_compat::js_json::from_value(&value.clone()).unwrap();
            let mut result = Map::new();
            result.insert(
                "aborted".to_owned().into(),
                Value::Bool(did_last_assistant_abort(&event)),
            );
            result.insert(
                "empty".to_owned().into(),
                Value::Bool(did_last_assistant_return_empty(&event)),
            );
            if let Some(message) = find_last_assistant_message(&event) {
                result.insert(
                    "stopReason".to_owned().into(),
                    pi_js_compat::js_json::to_value(&message.stop_reason).unwrap(),
                );
            }
            Value::Object(result)
        })
        .collect();
    assert_eq!(Value::Array(actual), output["output"]);
}

#[test]
fn matches_typescript_goal_goal_value_serialization() {
    let mut oracle = oracle();
    let value = json!({
        "objective": "ship it",
        "branch": "main",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "completedAt": "2026-01-02T00:00:00.000Z",
        "completionReason": "done",
        "continuationTurns": 0,
        "pausedAt": "2026-01-03T00:00:00.000Z",
        "pauseReason": "blocked",
        "reviewEvidence": [{"kind": "user", "text": "hello"}]
    });
    let output = oracle.request(&json!({
        "id": 1,
        "operation": "parse-goal",
        "cases": [{"kind": "parse-goal", "value": value}]
    }));
    let goal = parse_goal(&value).unwrap();
    assert_eq!(Value::Array(vec![goal_value(&goal)]), output["output"]);
}
