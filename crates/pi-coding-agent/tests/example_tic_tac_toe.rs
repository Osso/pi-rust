#[path = "../examples/extensions/tic_tac_toe.rs"]
mod example;
#[path = "support/extensions.rs"]
mod extension_support;
#[path = "support/extension_examples_b.rs"]
mod harness;
#[path = "support/extension_examples_a.rs"]
mod messages;
#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::sync::Arc;

fn details(board: [&str; 3], status: &str, turn: &str, row: usize, col: usize) -> Value {
    json!({"board":board.map(|line|line.chars().map(|c|c.to_string()).collect::<Vec<_>>()),
        "agentCursorRow":row,"agentCursorCol":col,"status":status,"currentTurn":turn})
}
fn restored(state: Value) -> Value {
    json!({"event":"append","message":{"role":"toolResult","toolCallId":"previous","toolName":"tic_tac_toe","content":[],"details":state,"isError":false,"timestamp":0}})
}
fn action(name: &str) -> Value {
    json!({"execute":"tic_tac_toe","params":{"action":name}})
}
fn factory() -> pi_coding_agent::core::extensions::ExtensionFactory {
    Arc::new(|api| {
        let result = example::tic_tac_toe_extension(api);
        Box::pin(async move { result })
    })
}

#[tokio::test(start_paused = true)]
async fn scripted_games_frames_tools_errors_and_persistence_match() {
    let mut cases = vec![
        json!({"event":"command","name":"tic-tac-toe","mode":"rpc"}),
        json!({"event":"session_start"}),
        json!({"event":"execute","toolName":"tic_tac_toe","params":{"action":"play"}}),
    ];
    for width in [18, 40, 80] {
        cases.push(json!({"event":"command","name":"tic-tac-toe","width":width,"gameScript":[
            {"before":"Pinned base prompt"},
            {"key":"\x1b[A"},{"key":"\x1b[A"},{"key":"\x1b[D"},{"key":"\x1b[D"},
            {"key":"\r"},{"key":"\x1b[B"},action("play"),action("move_right"),action("play"),{"key":"\r"},
            {"key":"\x1b[B"},{"key":" "},action("move_left"),action("move_up"),action("move_down"),action("move_right"),action("play"),
            {"execute":"tic_tac_toe_see_board","params":{}},{"width":25},{"key":"q"}
        ]}));
        cases.push(json!({"event":"entries"}));
    }
    // Restored O turns let the real tool reach O win and draw without changing game logic.
    for (board, row, col) in [(["OO ", "XX ", "X  "], 0, 2), (["XOX", "XOO", "OX "], 2, 2)] {
        cases.push(restored(details(board, "playing", "O", row, col)));
        cases.push(json!({"event":"command","name":"tic-tac-toe","width":40,"gameScript":[
            action("play"),action("play"),{"before":"No active game"},{"key":" "},{"key":"r"}
        ]}));
        cases.push(json!({"event":"execute","toolName":"tic_tac_toe_see_board","params":{}}));
        cases.push(json!({"event":"entries"}));
    }
    // Human win, cursor boundaries, quit/restart handling and completed-game reconstruction.
    cases.push(restored(details(["XX ", "OO ", "   "], "playing", "X", 2, 2)));
    cases.push(json!({"event":"command","name":"tic-tac-toe","width":40,"gameScript":[
        {"key":"\x1b[A"},{"key":"\x1b[C"},{"key":"\x1b[C"},{"key":"\r"},{"key":"z"},{"key":"r"}
    ]}));
    cases.push(json!({"event":"command","name":"tic-tac-toe","width":40,"keys":["\x1b"]}));
    cases.push(restored(details(["XXX", "OO ", "   "], "win_X", "X", 2, 2)));
    cases.push(json!({"event":"command","name":"tic-tac-toe","width":40,"keys":["Q"]}));
    cases.push(json!({"event":"execute","toolName":"tic_tac_toe_see_board","params":{}}));
    cases.push(json!({"event":"branch","index":0}));
    cases.push(json!({"event":"event","value":{"type":"session_tree"}}));
    cases.push(json!({"event":"execute","toolName":"tic_tac_toe_see_board","params":{}}));
    // Move tools remain legal outside a game and clamp independently of turn/status.
    for name in [
        "move_down",
        "move_down",
        "move_down",
        "move_right",
        "move_right",
        "move_right",
        "move_up",
        "move_left",
    ] {
        cases.push(json!({"event":"execute","toolName":"tic_tac_toe","params":{"action":name}}));
    }
    let actual = harness::compare(
        "tic-tac-toe",
        factory(),
        json!({"captureEffects":true,"virtualTimers":true,"cases":cases}),
    )
    .await;
    let effects = actual["effects"].as_array().unwrap();
    assert!(
        effects
            .iter()
            .any(|e| e[0] == "sendMessage" && e[1]["content"] == "Game over: Player O (agent) wins.")
    );
    assert!(
        effects
            .iter()
            .any(|e| e[0] == "sendMessage" && e[1]["content"] == "Game over: Player X (human) wins.")
    );
    assert!(
        effects
            .iter()
            .any(|e| e[0] == "sendMessage" && e[1]["content"] == "Game over: Draw.")
    );
    assert_eq!(actual["cases"][2]["result"]["error"], "It is not your turn.");
}

#[tokio::test]
async fn persisted_message_and_tool_renderers_match_complete_ansi_lines() {
    let mut steps = vec![];
    for board in [
        details(["   ", "   ", "   "], "playing", "O", 0, 0),
        details(["XXX", "OO ", "   "], "win_X", "X", 0, 0),
        details(["OO ", "XXX", "   "], "win_X", "X", 0, 0),
        details(["OO ", "   ", "XXX"], "win_X", "X", 0, 0),
        details(["XO ", "XO ", "X  "], "win_X", "X", 0, 0),
        details(["OX ", "OX ", " X "], "win_X", "X", 0, 0),
        details([" OX", " OX", "  X"], "win_X", "X", 0, 0),
        details([" OX", " XO", "X  "], "win_X", "X", 0, 0),
        details(["X X", " O ", "   "], "playing", "X", 2, 1),
        details(["   ", "   ", "   "], "unknown", "X", 0, 0),
        details(["OX ", " OX", "  O"], "win_O", "O", 0, 0),
        details(["XOX", "XOO", "OXO"], "draw", "O", 0, 0),
        json!(null),
    ] {
        for width in [12, 25, 40, 80] {
            for expanded in [false, true] {
                for kind in ["tic-tac-toe-move", "tic-tac-toe-game-over"] {
                    steps.push(json!({"op":"renderMessage","message":{"customType":kind,"content":"Persisted game","display":true,"details":board,"timestamp":0},"expanded":expanded,"pad":0,"width":width}));
                }
            }
            for name in ["tic_tac_toe", "tic_tac_toe_see_board"] {
                steps.push(json!({"op":"renderTool","name":name,"result":{"content":[{"type":"text","text":"Concrete result"}],"details":board},"width":width}));
            }
        }
    }
    messages::compare(
        "tic-tac-toe",
        messages::factory(example::tic_tac_toe_extension),
        json!({"steps":steps}),
    )
    .await;
    let mut cases = vec![];
    for name in ["tic_tac_toe", "tic_tac_toe_see_board"] {
        cases.push(json!({"event":"tool_metadata","toolName":name}));
        cases.push(json!({"event":"render_call","toolName":name,"params":{"action":"move_right"},"width":18}));
        for expanded in [false, true] {
            for error in [false, true] {
                cases.push(json!({"event":"render_result","toolName":name,"result":{"content":[],"details":null},"expanded":expanded,"isError":error,"width":40}));
            }
        }
    }
    harness::compare("tic-tac-toe", factory(), json!({"cases":cases})).await;
}

use extension_support as scheduler;

#[tokio::test(start_paused = true)]
async fn sibling_moves_finish_before_play_through_real_agent_scheduler() {
    use pi_agent_core::agent::{AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, StreamFn, agent_loop};
    use pi_ai::types::{AssistantContent, AssistantMessage, AssistantMessageEvent, DoneReason, StopReason, ToolCall};
    use pi_ai::utils::event_stream::create_assistant_message_event_stream;
    use pi_coding_agent::core::extensions::*;
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory(),
        &JsString::from(cwd),
        scheduler::event_bus(),
        runtime.clone(),
        &JsString::from("tic-tac-toe"),
    )
    .await
    .unwrap();
    let model_registry = pi_coding_agent::core::model_registry::ModelRegistry::in_memory(
        pi_coding_agent::core::auth_storage::AuthStorage::in_memory(serde_json::Map::new()),
    )
    .unwrap();
    let core_session = scheduler::session_manager(cwd);
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        core_session.clone(),
        Arc::new(model_registry),
        None,
    );
    scheduler::bind(&mut runner);
    let initial = details(["X  ", "   ", "   "], "playing", "O", 0, 0);
    core_session
        .lock()
        .unwrap()
        .append_message(serde_json::from_value(restored(initial)["message"].clone()).unwrap())
        .unwrap();
    runner
        .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
            reason: SessionStartReason::Startup,
            previous_session_file: None,
        }))
        .await;
    let tool = pi_coding_agent::core::extensions::wrapper::wrap_definition_tool(
        runner.get_tool_definition("tic_tac_toe").unwrap(),
        &runner,
    );
    let mut turn = AssistantMessage::empty("openai-responses", "fixture", "fixture", 0.0);
    turn.stop_reason = StopReason::ToolUse;
    turn.content = ["move_down", "move_right", "play"]
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            AssistantContent::ToolCall(ToolCall::new(
                index.to_string(),
                "tic_tac_toe",
                (json!({"action":action}).as_object().unwrap().clone())
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
            ))
        })
        .collect();
    let turn = Arc::new(std::sync::Mutex::new(Some(turn)));
    let stream_fn: StreamFn = Arc::new(move |_, _, _| {
        let turn = turn.lock().unwrap().take();
        Box::pin(async move {
            let (sender, stream) = create_assistant_message_event_stream();
            let (reason, message) = match turn {
                Some(turn) => (DoneReason::ToolUse, turn),
                None => (
                    DoneReason::Stop,
                    AssistantMessage::empty("openai-responses", "fixture", "fixture", 0.0),
                ),
            };
            sender.push(AssistantMessageEvent::Done { reason, message });
            Ok(stream)
        })
    });
    let model = serde_json::from_value(json!({"id":"fixture","name":"Fixture","api":"openai-responses","provider":"fixture","baseUrl":"https://invalid.test","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":8192,"maxTokens":1024})).unwrap();
    let mut config = AgentLoopConfig::new(
        model,
        Arc::new(|messages| Box::pin(async move { pi_agent_core::harness::messages::convert_to_llm(&messages) })),
    );
    config.tool_execution = pi_agent_core::harness::ToolExecutionMode::Parallel;
    let started = tokio::time::Instant::now();
    let mut stream = agent_loop(
        vec![
            serde_json::from_value(json!({"role":"user","content":"move down, right, then play","timestamp":0}))
                .unwrap(),
        ],
        AgentContext {
            system_prompt: JsString::default(),
            messages: vec![],
            tools: Some(vec![tool]),
        },
        config,
        None,
        Some(stream_fn),
    );
    let mut results = Vec::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::MessageEnd {
            message: AgentMessage::ToolResult(result),
        } = event.unwrap()
        {
            assert!(!result.is_error, "{}: {:?}", result.tool_call_id, result.content);
            results.push(result);
        }
    }
    assert_eq!(results.len(), 3);
    assert_eq!(
        results.iter().map(|r| r.tool_call_id.as_str()).collect::<Vec<_>>(),
        ["0", "1", "2"]
    );
    assert!(started.elapsed() >= std::time::Duration::from_millis(600));
    let final_state = results[2].details.as_ref().unwrap();
    assert_eq!(
        final_state["board"],
        pi_js_compat::js_json!([["X", " ", " "], [" ", "O", " "], [" ", " ", " "]])
    );
    assert_eq!(
        results[0].details.as_ref().unwrap()["agentCursorRow"],
        pi_js_compat::js_json!(1)
    );
    assert_eq!(
        results[0].details.as_ref().unwrap()["agentCursorCol"],
        pi_js_compat::js_json!(0)
    );
    assert_eq!(
        results[1].details.as_ref().unwrap()["agentCursorRow"],
        pi_js_compat::js_json!(1)
    );
    assert_eq!(
        results[1].details.as_ref().unwrap()["agentCursorCol"],
        pi_js_compat::js_json!(1)
    );
    assert_eq!(final_state["agentCursorRow"], pi_js_compat::js_json!(0));
    assert_eq!(final_state["agentCursorCol"], pi_js_compat::js_json!(0));
    assert_eq!(final_state["currentTurn"], pi_js_compat::js_json!("X"));
}
