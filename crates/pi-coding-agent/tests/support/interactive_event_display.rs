#[test]
fn interleaved_text_tool_and_retry_compaction_frames_match_frozen() {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let mut source = oracle::TsOracle::spawn("interactive-startup-oracle.mts", "packages/coding-agent/src");
    let mut mismatches = Vec::new();
    for case in [
        "mixed",
        "retry-empty",
        "compaction",
        "compaction-empty",
        "provider-retry",
        "replay-error",
        "replay-complete",
        "replay-aborted",
    ] {
        let root = tempfile::tempdir().unwrap();
        let skill = root.path().join("skill");
        std::fs::create_dir(&skill).unwrap();
        let skill = skill.join("SKILL.md");
        std::fs::write(
            &skill,
            "---\nname: post-start\ndescription: A startup-discovered skill\n---\nSkill body\n",
        )
        .unwrap();
        let request = serde_json::json!({"cwd":root.path(),"agentDir":root.path(),"skillPath":skill,"quiet":true,"verbose":false,"scoped":false,"restored":false,"watchChanges":false,"eventCase":case});
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::startup_capture_child",
                "--nocapture",
            ])
            .env("PI_INTERACTIVE_STARTUP_CASE", request.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}\n{}",
            String::from_utf8(output.stdout.clone()).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let native: serde_json::Value = serde_json::from_str(
            stdout
                .lines()
                .find_map(|line| line.strip_prefix("STARTUP_RESULT "))
                .unwrap(),
        )
        .unwrap();
        let expected = source.request(&native["request"]);
        assert_eq!(expected["ok"], true, "{case}: {expected}");
        for (step, (actual, expected)) in native["output"]["eventFrames"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["output"]["eventFrames"].as_array().unwrap())
            .enumerate()
        {
            if JsValue::from(actual) != JsValue::from(expected) {
                mismatches.push(format!("{case} step{step}: native={actual} frozen={expected}"));
            }
        }
        assert_eq!(
            native["output"]["eventFrames"].as_array().unwrap().len(),
            expected["output"]["eventFrames"].as_array().unwrap().len()
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn build_display_fixture_events(case: &str) -> Vec<AgentSessionEvent> {
    use crate::core::agent_session::{
        AutoRetryEndEvent, CompactionEndEvent, CompactionStartEvent, ProviderStreamRetryEvent,
    };
    if case == "retry-empty" {
        return vec![AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
            event_type: "auto_retry_end",
            success: false,
            attempt: 2,
            final_error: Some(JsString::default()),
        })];
    }
    if case == "provider-retry" {
        return [
            pi_js_compat::js_json!({"attempt":2,"maxAttempts":3,"delayMs":1250,"reason":"server busy"}),
            pi_js_compat::js_json!({"attempt":1,"reason":"connection reset"}),
            pi_js_compat::js_json!({"attempt":1,"fallbackTransport":"sse","reason":"socket unavailable"}),
        ]
        .into_iter()
        .map(|retry| {
            AgentSessionEvent::ProviderStreamRetry(ProviderStreamRetryEvent {
                event_type: "provider_stream_retry",
                retry: pi_js_compat::js_json::from_value(&retry).unwrap(),
            })
        })
        .collect();
    }
    if case == "compaction" || case == "compaction-empty" {
        return vec![
            AgentSessionEvent::CompactionStart(CompactionStartEvent {
                event_type: "compaction_start",
                reason: "manual".into(),
                source_hint: None,
            }),
            AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                event_type: "compaction_end",
                reason: "manual".into(),
                result: None,
                aborted: case == "compaction",
                will_retry: false,
                error_message: (case == "compaction-empty").then(JsString::default),
            })),
        ];
    }
    assert_eq!(case, "mixed", "unknown display event fixture");
    let mut initial = faux_assistant_message(faux_text(""), Default::default());
    initial.content.clear();
    initial.timestamp = 0.0;
    let mut partial = faux_assistant_message(faux_text("Hello"), Default::default());
    partial.timestamp = 0.0;
    partial.stop_reason = pi_ai::types::StopReason::ToolUse;
    let call:pi_ai::types::ToolCall=pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({"type":"toolCall","id":"call-display","name":"read","arguments":{"path":"fixture.txt"}})).unwrap();
    partial.content.push(pi_ai::types::AssistantContent::ToolCall(call));
    let mut tool_start = partial.clone();
    if let pi_ai::types::AssistantContent::Text(text) = &mut tool_start.content[0] {
        text.text = JsString::default();
    }
    vec![
        AgentSessionEvent::Agent(AgentEvent::MessageStart {
            message: AgentMessage::Assistant(Box::new(initial)),
        }),
        AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant(Box::new(tool_start.clone())),
            assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::ToolCallStart {
                content_index: 1,
                partial: tool_start,
            }),
        }),
        AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
            message: AgentMessage::Assistant(Box::new(partial.clone())),
            assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "Hello".into(),
                partial: partial.clone(),
            }),
        }),
        AgentSessionEvent::Agent(AgentEvent::MessageEnd {
            message: AgentMessage::Assistant(Box::new(partial)),
        }),
    ]
}

fn build_display_replay_messages(case: &str) -> Vec<AgentMessage> {
    let timestamp = 1_792_843_696_000.0;
    let mut assistant = faux_assistant_message(faux_text("Reading fixture"), Default::default());
    assistant.timestamp = timestamp;
    assistant.content.push(pi_ai::types::AssistantContent::ToolCall(
        pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({"type":"toolCall","id":"call-replay","name":"read","arguments":{"path":"fixture.txt"}})).unwrap()
    ));
    match case {
        "replay-error" => {
            assistant.stop_reason = pi_ai::types::StopReason::Error;
            assistant.error_message = Some(JsString::default());
            vec![assistant.into()]
        }
        "replay-aborted" => {
            assistant.stop_reason = pi_ai::types::StopReason::Aborted;
            vec![assistant.into()]
        }
        "replay-complete" => {
            assistant.stop_reason = pi_ai::types::StopReason::ToolUse;
            let result: AgentMessage = pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({"role":"toolResult","toolCallId":"call-replay","toolName":"read","content":[{"type":"text","text":"File body"}],"isError":false,"timestamp":timestamp+1000.0})).unwrap();
            vec![assistant.into(), result]
        }
        _ => panic!("unknown replay fixture: {case}"),
    }
}

fn capture_event_display_frame(mode: &InteractiveMode, terminal: &Rc<RefCell<CaptureTerminal>>) -> serde_json::Value {
    let render = |container: &mut dyn Component, width| {
        container
            .render(width)
            .into_iter()
            .map(|line| line.units().to_vec())
            .collect::<Vec<_>>()
    };
    serde_json::json!({
        "chat":render(&mut *mode.chat_container.borrow_mut(), 120),
        "tail":render(&mut *mode.transcript_tail_container.borrow_mut(), 120),
        "status":render(&mut *mode.status_container.borrow_mut(), 120),
        "narrow": {
            "chat":render(&mut *mode.chat_container.borrow_mut(), 25),
            "tail":render(&mut *mode.transcript_tail_container.borrow_mut(), 25),
            "status":render(&mut *mode.status_container.borrow_mut(), 25),
        },
        "progress":terminal.borrow().progress,
    })
}
