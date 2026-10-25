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
        "replay-thinking",
        "live-thinking",
        "editor-ctrl-c-spaced",
        "editor-follow-up-idle",
        "editor-follow-up-paste",
        "editor-dequeue-empty",
        "editor-dequeue-one",
        "editor-dequeue-many",
        "editor-dequeue-whitespace",
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
    if case == "live-thinking" {
        return build_live_thinking_events();
    }
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
        "replay-thinking" => {
            assistant.content = vec![
                pi_js_compat::js_json::from_value(
                    &pi_js_compat::js_json!({"type":"thinking","thinking":"Reasoning before the answer"}),
                )
                .unwrap(),
                pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({"type":"text","text":"The answer"}))
                    .unwrap(),
            ];
            assistant.stop_reason = pi_ai::types::StopReason::Stop;
            vec![assistant.into()]
        }
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

async fn drive_editor_display_fixture(
    mode: &Rc<InteractiveMode>,
    terminal: &Rc<RefCell<CaptureTerminal>>,
    case: &str,
    request: &mut serde_json::Value,
) -> Vec<serde_json::Value> {
    if case.starts_with("editor-follow-up-") {
        let inputs = Rc::new(RefCell::new(Vec::<JsString>::new()));
        let observed = Rc::clone(&inputs);
        *mode.on_input_callback.borrow_mut() = Some(Rc::new(move |text| observed.borrow_mut().push(text)));
        request["observeInput"] = serde_json::json!(true);
        request["editorActions"] = serde_json::json!(["follow-up"]);
        if case == "editor-follow-up-paste" {
            let paste = (0..12)
                .map(|line| format!("Payload line {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            request["pasteText"] = serde_json::json!(paste);
            mode.default_editor
                .borrow_mut()
                .handle_input(&JsString::from(format!("\x1b[200~{paste}\x1b[201~")));
        } else {
            request["editorText"] = serde_json::json!("  Next question  ");
            mode.editor()
                .borrow_mut()
                .set_text(&JsString::from("  Next question  "))
                .unwrap();
        }
        mode.handle_follow_up().await.unwrap();
        mode.drain_actions().await;
        tokio::task::yield_now().await;
        let mut frame = capture_event_display_frame(mode, terminal);
        frame["inputs"] = serde_json::json!(
            inputs
                .borrow()
                .iter()
                .map(|text| text.units().to_vec())
                .collect::<Vec<_>>()
        );
        frame["queuedFollowUp"] = serde_json::json!(
            mode.session()
                .get_follow_up_messages()
                .iter()
                .map(|text| text.units().to_vec())
                .collect::<Vec<_>>()
        );
        return vec![frame];
    }
    if case == "replay-thinking" || case == "live-thinking" {
        let actions = ["toggle-thinking", "toggle-thinking"];
        request["editorActions"] = serde_json::json!(actions);
        return drive_captured_editor_actions(mode, terminal, &actions).await;
    }
    if case == "editor-ctrl-c-spaced" {
        let actions = ["ctrl-c", "set-second-line", "wait-750", "ctrl-c"];
        request["editorActions"] = serde_json::json!(actions);
        return drive_captured_editor_actions(mode, terminal, &actions).await;
    }
    if !case.starts_with("editor-dequeue-") {
        return Vec::new();
    }
    let text = if case == "editor-dequeue-whitespace" {
        " \n "
    } else {
        "Unsent text"
    };
    mode.editor().borrow_mut().set_text(&JsString::from(text)).unwrap();
    let follow_up = match case {
        "editor-dequeue-empty" => vec![],
        "editor-dequeue-many" => vec!["Queued first", "Queued second"],
        _ => vec!["Queued first"],
    };
    request["editorText"] = serde_json::json!(text);
    request["followUp"] = serde_json::json!(follow_up);
    for text in follow_up {
        mode.session().follow_up(&JsString::from(text), &[]).await.unwrap();
    }
    if case == "editor-dequeue-many" {
        mode.compaction_queued_messages.borrow_mut().extend([
            CompactionQueuedMessage {
                text: "Compaction steer".into(),
                mode: "steer",
            },
            CompactionQueuedMessage {
                text: "Compaction follow-up".into(),
                mode: "followUp",
            },
        ]);
        request["compactionQueue"] = serde_json::json!([{"text":"Compaction steer","mode":"steer"},{"text":"Compaction follow-up","mode":"followUp"}]);
    }
    mode.update_pending_messages_display();
    request["editorActions"] = serde_json::json!(["dequeue"]);
    mode.handle_dequeue().unwrap();
    vec![capture_event_display_frame(mode, terminal)]
}

fn build_live_thinking_events() -> Vec<AgentSessionEvent> {
    let AgentMessage::Assistant(message) = build_display_replay_messages("replay-thinking").pop().unwrap() else {
        unreachable!()
    };
    let mut empty = (*message).clone();
    empty.content.clear();
    let mut thinking = (*message).clone();
    thinking.content.truncate(1);
    vec![
        AgentSessionEvent::Agent(AgentEvent::MessageStart { message: empty.into() }),
        AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
            message: thinking.clone().into(),
            assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "Reasoning before the answer".into(),
                partial: thinking,
            }),
        }),
        AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
            message: (*message).clone().into(),
            assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::TextDelta {
                content_index: 1,
                delta: "The answer".into(),
                partial: *message,
            }),
        }),
    ]
}

async fn drive_captured_editor_actions(
    mode: &InteractiveMode,
    terminal: &Rc<RefCell<CaptureTerminal>>,
    actions: &[&str],
) -> Vec<serde_json::Value> {
    let mut frames = Vec::new();
    for action in actions {
        match *action {
            "toggle-thinking" => mode.toggle_thinking_block_visibility(),
            "ctrl-c" => mode.handle_ctrl_c().unwrap(),
            "set-second-line" => mode
                .editor()
                .borrow_mut()
                .set_text(&JsString::from("Second line"))
                .unwrap(),
            "wait-750" => tokio::time::sleep(Duration::from_millis(750)).await,
            _ => panic!("unknown captured editor action: {action}"),
        }
        frames.push(capture_event_display_frame(mode, terminal));
    }
    frames
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
        "pending":render(&mut *mode.pending_messages_container.borrow_mut(), 120),
        "editorText":mode.editor().borrow().get_text().unwrap().units(),
        "hideThinking":mode.settings(|settings| settings.get_hide_thinking_block()),
        "narrow": {
            "chat":render(&mut *mode.chat_container.borrow_mut(), 25),
            "tail":render(&mut *mode.transcript_tail_container.borrow_mut(), 25),
            "status":render(&mut *mode.status_container.borrow_mut(), 25),
            "pending":render(&mut *mode.pending_messages_container.borrow_mut(), 25),
        },
        "progress":terminal.borrow().progress,
    })
}
