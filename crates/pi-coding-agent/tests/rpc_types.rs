use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::{ImageContent, Modality, Model, ModelCost, ProviderId};
use pi_coding_agent::core::source_info::{SourceInfo, SourceOrigin, SourceScope};
use pi_coding_agent::modes::rpc::{
    ExtensionUiRequestTag, ExtensionUiResponseTag, NotifyType, QueueMode, RpcCommand, RpcCommandBody,
    RpcExtensionUiRequest, RpcExtensionUiRequestPayload, RpcExtensionUiResponse, RpcExtensionUiResponsePayload,
    RpcResponse, RpcSessionState, RpcSlashCommand, RpcSlashCommandSource, StreamingBehavior, WidgetPlacement,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

fn parse(line: &str) -> RpcCommand {
    serde_json::from_str(line).expect("command line must parse")
}

fn serialize_command(body: RpcCommandBody, id: Option<&str>) -> String {
    serde_json::to_string(&RpcCommand::new(body, id.map(JsString::from))).expect("command must serialize")
}

fn source_info() -> SourceInfo {
    SourceInfo {
        path: JsString::from("/tmp/command.ts"),
        source: JsString::from("test"),
        scope: SourceScope::Project,
        origin: SourceOrigin::TopLevel,
        base_dir: None,
    }
}

fn model() -> Model {
    Model {
        properties: Default::default(),
        id: "claude-sonnet-4-5".to_owned(),
        name: "Claude Sonnet 4.5".into(),
        api: "anthropic-messages".to_owned(),
        provider: ProviderId::from("anthropic"),
        base_url: JsString::from("https://api.anthropic.com"),
        reasoning: true,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: pi_ai::types::ModelCostRates {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
        },
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 64_000.0,
        headers: None,
        compat: None,
    }
}

#[test]
fn parses_every_prompting_command() {
    let prompt = parse(
        r#"{"id":"a","type":"prompt","message":"hi","images":[{"type":"image","data":"AA==","mimeType":"image/png"}],"streamingBehavior":"followUp"}"#,
    );
    assert_eq!(prompt.id.as_ref(), Some(&JsString::from("a")));
    assert_eq!(
        prompt.body,
        RpcCommandBody::Prompt {
            message: "hi".into(),
            images: Some(vec![ImageContent::new("AA==", "image/png")]),
            streaming_behavior: Some(StreamingBehavior::FollowUp),
        }
    );

    assert_eq!(
        parse(r#"{"type":"steer","message":"s"}"#).body,
        RpcCommandBody::Steer {
            message: "s".into(),
            images: None,
        }
    );
    assert_eq!(
        parse(r#"{"type":"follow_up","message":"f"}"#).body,
        RpcCommandBody::FollowUp {
            message: "f".into(),
            images: None,
        }
    );
    assert_eq!(parse(r#"{"type":"abort"}"#).body, RpcCommandBody::Abort);
    assert_eq!(parse(r#"{"type":"interrupt"}"#).body, RpcCommandBody::Interrupt);
    assert_eq!(
        parse(r#"{"type":"new_session","parentSession":"/tmp/parent.jsonl"}"#).body,
        RpcCommandBody::NewSession {
            parent_session: Some("/tmp/parent.jsonl".into()),
        }
    );
    assert_eq!(
        parse(r#"{"type":"new_session"}"#).body,
        RpcCommandBody::NewSession { parent_session: None }
    );
}

#[test]
fn parses_every_state_model_and_thinking_command() {
    assert_eq!(parse(r#"{"type":"get_state"}"#).body, RpcCommandBody::GetState);
    assert_eq!(
        parse(r#"{"type":"set_model","provider":"anthropic","modelId":"m"}"#).body,
        RpcCommandBody::SetModel {
            provider: "anthropic".into(),
            model_id: "m".into(),
        }
    );
    assert_eq!(parse(r#"{"type":"cycle_model"}"#).body, RpcCommandBody::CycleModel);
    assert_eq!(
        parse(r#"{"type":"get_available_models"}"#).body,
        RpcCommandBody::GetAvailableModels
    );
    assert_eq!(
        parse(r#"{"type":"set_thinking_level","level":"xhigh"}"#).body,
        RpcCommandBody::SetThinkingLevel {
            level: ThinkingLevel::XHigh,
        }
    );
    assert_eq!(
        parse(r#"{"type":"cycle_thinking_level"}"#).body,
        RpcCommandBody::CycleThinkingLevel
    );
    // Frozen rpc-types.ts:20-79 has no available-thinking-level command.
    assert!(serde_json::from_str::<RpcCommand>(r#"{"type":"get_available_thinking_levels"}"#).is_err());
}

#[test]
fn parses_every_queue_compaction_retry_and_bash_command() {
    assert_eq!(
        parse(r#"{"type":"set_steering_mode","mode":"one-at-a-time"}"#).body,
        RpcCommandBody::SetSteeringMode {
            mode: QueueMode::OneAtATime,
        }
    );
    assert_eq!(
        parse(r#"{"type":"set_follow_up_mode","mode":"all"}"#).body,
        RpcCommandBody::SetFollowUpMode { mode: QueueMode::All }
    );
    assert_eq!(
        parse(r#"{"type":"compact","customInstructions":"focus"}"#).body,
        RpcCommandBody::Compact {
            custom_instructions: Some(JsString::from("focus")),
        }
    );
    assert_eq!(
        parse(r#"{"type":"compact"}"#).body,
        RpcCommandBody::Compact {
            custom_instructions: None,
        }
    );
    assert_eq!(
        parse(r#"{"type":"set_auto_compaction","enabled":false}"#).body,
        RpcCommandBody::SetAutoCompaction { enabled: false }
    );
    assert_eq!(
        parse(r#"{"type":"set_auto_retry","enabled":true}"#).body,
        RpcCommandBody::SetAutoRetry { enabled: true }
    );
    assert_eq!(parse(r#"{"type":"abort_retry"}"#).body, RpcCommandBody::AbortRetry);
    assert_eq!(
        parse(r#"{"type":"bash","command":"echo hi","excludeFromContext":true}"#).body,
        RpcCommandBody::Bash {
            command: JsString::from("echo hi"),
            exclude_from_context: Some(true),
        }
    );
    assert_eq!(parse(r#"{"type":"abort_bash"}"#).body, RpcCommandBody::AbortBash);
}

#[test]
fn parses_every_session_message_and_command_query() {
    assert_eq!(
        parse(r#"{"type":"get_session_stats"}"#).body,
        RpcCommandBody::GetSessionStats
    );
    assert_eq!(
        parse(r#"{"type":"export_html","outputPath":"/tmp/out.html"}"#).body,
        RpcCommandBody::ExportHtml {
            output_path: Some("/tmp/out.html".into()),
        }
    );
    assert_eq!(
        parse(r#"{"type":"switch_session","sessionPath":"/tmp/s.jsonl"}"#).body,
        RpcCommandBody::SwitchSession {
            session_path: "/tmp/s.jsonl".into(),
        }
    );
    assert_eq!(
        parse(r#"{"type":"fork","entryId":"e1"}"#).body,
        RpcCommandBody::Fork { entry_id: "e1".into() }
    );
    assert_eq!(parse(r#"{"type":"clone"}"#).body, RpcCommandBody::Clone);
    assert_eq!(
        parse(r#"{"type":"get_fork_messages"}"#).body,
        RpcCommandBody::GetForkMessages
    );
    assert_eq!(
        parse(r#"{"type":"get_entries","since":"e0"}"#).body,
        RpcCommandBody::GetEntries {
            since: Some("e0".into()),
        }
    );
    assert_eq!(
        parse(r#"{"type":"get_entries"}"#).body,
        RpcCommandBody::GetEntries { since: None }
    );
    assert_eq!(parse(r#"{"type":"get_tree"}"#).body, RpcCommandBody::GetTree);
    assert_eq!(
        parse(r#"{"type":"get_last_assistant_text"}"#).body,
        RpcCommandBody::GetLastAssistantText
    );
    assert_eq!(
        parse(r#"{"type":"set_session_name","name":"Name"}"#).body,
        RpcCommandBody::SetSessionName {
            name: JsString::from("Name"),
        }
    );
    assert_eq!(parse(r#"{"type":"get_messages"}"#).body, RpcCommandBody::GetMessages);
    assert_eq!(parse(r#"{"type":"get_commands"}"#).body, RpcCommandBody::GetCommands);
}

#[test]
fn serializes_command_bodies_with_ts_field_order_and_omits_undefined_fields() {
    assert_eq!(
        serialize_command(
            RpcCommandBody::Prompt {
                message: "hi".into(),
                images: None,
                streaming_behavior: None,
            },
            Some("req_1"),
        ),
        r#"{"type":"prompt","message":"hi","id":"req_1"}"#
    );
    assert_eq!(serialize_command(RpcCommandBody::Abort, None), r#"{"type":"abort"}"#);
    assert_eq!(
        serialize_command(
            RpcCommandBody::SetModel {
                provider: "anthropic".into(),
                model_id: "m".into(),
            },
            None,
        ),
        r#"{"type":"set_model","provider":"anthropic","modelId":"m"}"#
    );
    assert_eq!(
        serialize_command(
            RpcCommandBody::SetThinkingLevel {
                level: ThinkingLevel::High,
            },
            None,
        ),
        r#"{"type":"set_thinking_level","level":"high"}"#
    );
    assert_eq!(
        serialize_command(
            RpcCommandBody::Bash {
                command: JsString::from("ls"),
                exclude_from_context: Some(false),
            },
            None,
        ),
        r#"{"type":"bash","command":"ls","excludeFromContext":false}"#
    );
    assert_eq!(
        serialize_command(
            RpcCommandBody::ExportHtml {
                output_path: Some("/tmp/a.html".into()),
            },
            None,
        ),
        r#"{"type":"export_html","outputPath":"/tmp/a.html"}"#
    );
}

#[test]
fn serializes_session_state_in_ts_field_order_and_omits_undefined_fields() {
    let full = RpcSessionState {
        model: Some(model()),
        thinking_level: ThinkingLevel::Medium,
        is_streaming: false,
        is_compacting: true,
        steering_mode: QueueMode::All,
        follow_up_mode: QueueMode::OneAtATime,
        session_file: Some("/tmp/session.jsonl".into()),
        session_id: "session-1".into(),
        session_name: Some(JsString::from("Name")),
        auto_compaction_enabled: true,
        message_count: 3.0,
        pending_message_count: 2.0,
    };
    let value: Value = serde_json::to_value(&full).expect("state must serialize");
    let keys: Vec<&str> = value
        .as_object()
        .expect("state must be an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec![
            "model",
            "thinkingLevel",
            "isStreaming",
            "isCompacting",
            "steeringMode",
            "followUpMode",
            "sessionFile",
            "sessionId",
            "sessionName",
            "autoCompactionEnabled",
            "messageCount",
            "pendingMessageCount",
        ]
    );
    assert_eq!(value["thinkingLevel"], json!("medium"));
    assert_eq!(value["steeringMode"], json!("all"));
    assert_eq!(value["followUpMode"], json!("one-at-a-time"));
    assert_eq!(value["messageCount"], json!(3));
    assert_eq!(value["model"]["provider"], json!("anthropic"));

    let minimal = RpcSessionState {
        model: None,
        session_file: None,
        session_name: None,
        ..full
    };
    let value: Value = serde_json::to_value(&minimal).expect("state must serialize");
    assert_eq!(value.as_object().expect("state must be an object").keys().len(), 9);
    assert!(value.get("model").is_none());
    assert!(value.get("sessionFile").is_none());
    assert!(value.get("sessionName").is_none());
}

#[test]
fn serializes_slash_commands_and_omits_absent_descriptions() {
    let command = RpcSlashCommand {
        name: JsString::from("skill:review"),
        description: Some(JsString::from("Review code")),
        source: RpcSlashCommandSource::Skill,
        source_info: source_info(),
    };
    assert_eq!(
        serde_json::to_string(&command).expect("command must serialize"),
        r#"{"name":"skill:review","description":"Review code","source":"skill","sourceInfo":{"path":"/tmp/command.ts","source":"test","scope":"project","origin":"top-level"}}"#
    );

    let command = RpcSlashCommand {
        description: None,
        source: RpcSlashCommandSource::Extension,
        ..command
    };
    assert_eq!(
        serde_json::to_string(&command).expect("command must serialize"),
        r#"{"name":"skill:review","source":"extension","sourceInfo":{"path":"/tmp/command.ts","source":"test","scope":"project","origin":"top-level"}}"#
    );
}

#[test]
fn serializes_response_envelopes_in_ts_field_order() {
    assert_eq!(
        serde_json::to_string(&RpcResponse::success(Some(JsString::from("req_1")), "prompt"))
            .expect("response must serialize"),
        r#"{"id":"req_1","type":"response","command":"prompt","success":true}"#
    );
    assert_eq!(
        serde_json::to_string(&RpcResponse::success(None, "abort")).expect("response must serialize"),
        r#"{"type":"response","command":"abort","success":true}"#
    );
    assert_eq!(
        serde_json::to_string(&RpcResponse::success_with_data(
            Some(JsString::from("req_2")),
            "cycle_model",
            JsValue::from(Value::Null),
        ))
        .expect("response must serialize"),
        r#"{"id":"req_2","type":"response","command":"cycle_model","success":true,"data":null}"#
    );
    assert_eq!(
        serde_json::to_string(&RpcResponse::error(Some(JsString::from("req_3")), "fork", "boom",))
            .expect("response must serialize"),
        r#"{"id":"req_3","type":"response","command":"fork","success":false,"error":"boom"}"#
    );
    assert_eq!(
        serde_json::to_string(&RpcResponse::error(None, "parse", "bad json")).expect("response must serialize"),
        r#"{"type":"response","command":"parse","success":false,"error":"bad json"}"#
    );
}

#[test]
fn parses_response_lines() {
    let response: RpcResponse = serde_json::from_str(
        r#"{"id":"req_1","type":"response","command":"get_state","success":false,"error":"nope"}"#,
    )
    .expect("response must parse");
    assert_eq!(response.id.as_ref(), Some(&JsString::from("req_1")));
    assert_eq!(response.command.as_ref(), Some(&JsString::from("get_state")));
    assert!(!response.success);
    assert_eq!(response.error.as_ref(), Some(&JsString::from("nope")));
    assert_eq!(response.kind, pi_coding_agent::modes::rpc::RpcResponseTag::Response);
}

#[test]
fn serializes_every_extension_ui_request_in_ts_field_order() {
    let request = |payload| RpcExtensionUiRequest {
        kind: ExtensionUiRequestTag::ExtensionUiRequest,
        id: "ui-1".into(),
        payload,
    };

    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::Select {
            title: "Pick".into(),
            options: vec!["a".into(), "b".into()],
            timeout: Some(100.0),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"select","title":"Pick","options":["a","b"],"timeout":100}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::Confirm {
            title: "Sure?".into(),
            message: "Continue".into(),
            timeout: None,
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"confirm","title":"Sure?","message":"Continue"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::Input {
            title: "Name".into(),
            placeholder: Some("type".into()),
            timeout: None,
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"input","title":"Name","placeholder":"type"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::Editor {
            title: "Edit".into(),
            prefill: Some("text".into()),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"editor","title":"Edit","prefill":"text"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::Notify {
            message: "Done".into(),
            notify_type: Some(NotifyType::Warning),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"notify","message":"Done","notifyType":"warning"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::SetStatus {
            status_key: "k".into(),
            status_text: None,
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"setStatus","statusKey":"k"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::SetWidget {
            widget_key: "w".into(),
            widget_lines: Some(vec!["line".into()]),
            widget_placement: Some(WidgetPlacement::AboveEditor),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"setWidget","widgetKey":"w","widgetLines":["line"],"widgetPlacement":"aboveEditor"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::SetTitle {
            title: "Title".into(),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"setTitle","title":"Title"}"#
    );
    assert_eq!(
        serde_json::to_string(&request(RpcExtensionUiRequestPayload::SetEditorText {
            text: "text".into(),
        }))
        .expect("request must serialize"),
        r#"{"type":"extension_ui_request","id":"ui-1","method":"set_editor_text","text":"text"}"#
    );
}

#[test]
fn set_model_command_keeps_raw_string_fields_on_the_wire() {
    // Frozen rpc-types.ts:31 and client :263 only forward these strings.
    let wire =
        pi_js_compat::js_json::parse_lossless(r#"{"type":"set_model","provider":"p\ud800","modelId":"i\udfff"}"#)
            .unwrap();
    let command: RpcCommand = pi_js_compat::js_json::from_value(&wire).unwrap();
    assert_eq!(pi_js_compat::js_json::to_value(&command).unwrap(), wire);
}

#[test]
fn ui_cancellation_requires_the_declared_true_literal() {
    // Frozen rpc-types.ts:276 permits cancelled:true, not an arbitrary boolean.
    assert!(
        serde_json::from_str::<RpcExtensionUiResponse>(
            r#"{"type":"extension_ui_response","id":"ui","cancelled":false}"#
        )
        .is_err()
    );
    let invalid = RpcExtensionUiResponse {
        kind: ExtensionUiResponseTag::ExtensionUiResponse,
        id: "ui".into(),
        payload: RpcExtensionUiResponsePayload::Cancelled { cancelled: false },
    };
    assert!(serde_json::to_string(&invalid).is_err());
}

#[test]
fn declared_ui_strings_preserve_every_utf16_unit_when_decoded() {
    // Frozen rpc-types.ts:231-266 declares strings for every UI text/id field.
    for line in [
        r#"{"type":"extension_ui_request","id":"ui-\ud800","method":"select","title":"\udfff","options":["\ud800"],"timeout":1.5}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"confirm","title":"\ud800","message":"\udfff"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"input","title":"\ud800","placeholder":"\udfff"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"editor","title":"\ud800","prefill":"\udfff"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"notify","message":"\ud800","notifyType":"error"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"setStatus","statusKey":"\ud800","statusText":"\udfff"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"setWidget","widgetKey":"\ud800","widgetLines":["\udfff"],"widgetPlacement":"belowEditor"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"setTitle","title":"\ud800"}"#,
        r#"{"type":"extension_ui_request","id":"ui","method":"set_editor_text","text":"\udfff"}"#,
    ] {
        let wire = pi_js_compat::js_json::parse_lossless(line).unwrap();
        let request: RpcExtensionUiRequest = pi_js_compat::js_json::from_value(&wire).unwrap();
        assert_eq!(pi_js_compat::js_json::to_value(&request).unwrap(), wire);
    }
    let wire =
        pi_js_compat::js_json::parse_lossless(r#"{"type":"extension_ui_response","id":"\udfff","value":"\ud800"}"#)
            .unwrap();
    let response: RpcExtensionUiResponse = pi_js_compat::js_json::from_value(&wire).unwrap();
    assert_eq!(pi_js_compat::js_json::to_value(&response).unwrap(), wire);
}

#[test]
fn session_count_numbers_follow_the_declared_number_type() {
    // Frozen rpc-types.ts:115-116 declares number, not integer counts.
    let wire = pi_js_compat::js_json::parse_lossless(r#"{"thinkingLevel":"off","isStreaming":false,"isCompacting":false,"steeringMode":"all","followUpMode":"all","sessionId":"s","autoCompactionEnabled":true,"messageCount":1.5,"pendingMessageCount":-2}"#).unwrap();
    let state: RpcSessionState = pi_js_compat::js_json::from_value(&wire).unwrap();
    assert_eq!(pi_js_compat::js_json::to_value(&state).unwrap(), wire);
}

#[test]
fn parses_every_extension_ui_response_shape() {
    let value: RpcExtensionUiResponse =
        serde_json::from_str(r#"{"type":"extension_ui_response","id":"ui-1","value":"chosen"}"#)
            .expect("response must parse");
    assert_eq!(value.kind, ExtensionUiResponseTag::ExtensionUiResponse);
    assert_eq!(value.id, "ui-1");
    assert_eq!(
        value.payload,
        RpcExtensionUiResponsePayload::Value { value: "chosen".into() }
    );

    let confirmed: RpcExtensionUiResponse =
        serde_json::from_str(r#"{"type":"extension_ui_response","id":"ui-2","confirmed":true}"#)
            .expect("response must parse");
    assert_eq!(
        confirmed.payload,
        RpcExtensionUiResponsePayload::Confirmed { confirmed: true }
    );

    let cancelled: RpcExtensionUiResponse =
        serde_json::from_str(r#"{"type":"extension_ui_response","id":"ui-3","cancelled":true}"#)
            .expect("response must parse");
    assert_eq!(
        cancelled.payload,
        RpcExtensionUiResponsePayload::Cancelled { cancelled: true }
    );
}
