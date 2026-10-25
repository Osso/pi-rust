use pi_agent_core::agent::AgentMessage;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::session_manager::{FileEntry, SessionEntry, parse_session_entries};
use pi_coding_agent::core::session_tool_output::serialize_session_entry;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::parse_lossless;

#[test]
fn metadata_name_lookup_encodes_only_at_sql_binding() {
    use pi_coding_agent::core::session_control_db::find_active_session_metadata_by_name;
    use pi_coding_agent::core::session_manager::{SessionManager, SetMetadataControlDbPathOptions};
    use pi_js_compat::js_json::JsString;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_str().unwrap();
    let db = directory.path().join("control.sqlite");
    let db = db.to_str().unwrap();
    for unit in [0xd800, 0xdfff] {
        let mut manager = SessionManager::create(&JsString::from(root), Some(&JsString::from(root)), None).unwrap();
        manager
            .set_metadata_control_db_path(Some(&JsString::from(db)), SetMetadataControlDbPathOptions::default())
            .unwrap();
        manager
            .set_session_name(Some(&JsString::from_utf16(vec![unit])))
            .unwrap();
    }
    let name = JsString::from_utf16(vec![0xd800]);
    let rows = find_active_session_metadata_by_name(db, &name).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.name.as_ref() == Some(&JsString::from("�"))));
    assert_eq!(name.units(), &[0xd800]);
}

#[test]
fn shared_channel_encodes_raw_body_only_at_sql_binding() {
    use pi_coding_agent::core::session_control_db::{
        PostSharedChannelMessageInput, RuntimeMailboxAddress, list_shared_channel_messages_after,
        post_shared_channel_message,
    };
    use pi_js_compat::js_json::JsString;
    let directory = tempfile::tempdir().unwrap();
    let db = directory.path().join("control.sqlite");
    let db = db.to_str().unwrap();
    let body = JsString::from_utf16(vec![0x20, 0xd800, 0x78, 0xdfff, 0x20]);
    let input = PostSharedChannelMessageInput {
        body: body.clone(),
        sender: RuntimeMailboxAddress {
            session_id: "session".into(),
            agent_id: None,
        },
    };
    let id = post_shared_channel_message(db, &input).unwrap();
    assert_eq!(input.body, body);
    let rows = list_shared_channel_messages_after(db, 0, 10, id).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].body, "�x�");
}

#[test]
fn steering_and_cancellation_params_keep_raw_human_content() {
    use pi_coding_agent::bundled_extensions::agents_core::control_runtime::{CancelAgentParams, SteerAgentParams};
    let cancel: CancelAgentParams = serde_json::from_str(r#"{"agentId":"agent_1","reason":"\ud800"}"#).unwrap();
    assert_eq!(cancel.reason.unwrap().units(), &[0xd800]);
    let wire =
        r#"{"agentId":"agent_1","message":"\udfff","fileRefs":[{"path":"\ud800","label":"\udfff","\ud800":"\udfff"}]}"#;
    let steer: SteerAgentParams = serde_json::from_str(wire).unwrap();
    assert_eq!(steer.message.units(), &[0xdfff]);
    assert_eq!(steer.file_refs.unwrap(), parse_lossless(wire).unwrap()["fileRefs"]);
}

#[tokio::test]
async fn custom_find_operation_receives_original_pattern_units() {
    use pi_agent_core::agent::types::AgentTool;
    use pi_ai::request::BoxFuture;
    use pi_coding_agent::core::tools::find::{FindOperations, FindTool, FindToolOptions};
    use pi_js_compat::js_json::JsString;
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    struct FindPattern(Mutex<Option<JsString>>);
    impl FindOperations for FindPattern {
        fn exists<'a>(&'a self, _path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
            Box::pin(async { Ok(true) })
        }
        fn supports_glob(&self) -> bool {
            true
        }
        fn glob<'a>(
            &'a self,
            pattern: &'a JsString,
            _cwd: &'a JsString,
            _ignore: &'a [JsString],
            _limit: f64,
        ) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
            *self.0.lock().unwrap() = Some(pattern.clone());
            Box::pin(async { Ok(vec!["/tmp/hit.txt".into()]) })
        }
    }
    let operations = Arc::new(FindPattern(Mutex::new(None)));
    let tool = FindTool::with_options(
        "/tmp",
        FindToolOptions {
            operations: Some(operations.clone()),
        },
    );
    let result = tool
        .execute(
            "raw-pattern".into(),
            parse_lossless(r#"{"pattern":"*\ud800\udfff\ud800"}"#).unwrap(),
            None,
            std::sync::Arc::new(|_| Ok(())),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        operations.0.lock().unwrap().as_ref().unwrap().units(),
        &[0x2a, 0xd800, 0xdfff, 0xd800]
    );
    assert_eq!(
        result.content[0],
        pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new("hit.txt"))
    );
}

#[test]
fn session_manager_error_retains_native_message_before_display() {
    use pi_coding_agent::core::session_manager::SessionManagerError;
    use pi_js_compat::js_json::JsString;
    let error = SessionManagerError::new(JsString::from_utf16(vec![0xd800, 0x78, 0xdfff]));
    assert_eq!(error.message().units(), &[0xd800, 0x78, 0xdfff]);
    assert_eq!(error.to_string(), "�x�");
}

#[tokio::test]
async fn custom_write_operation_error_keeps_raw_units() {
    use pi_agent_core::agent::types::AgentTool;
    use pi_ai::request::BoxFuture;
    use pi_coding_agent::core::extensions::types::HandlerError;
    use pi_coding_agent::core::tools::write::{WriteOperations, WriteTool, WriteToolOptions};
    use pi_js_compat::js_json::JsString;
    use std::{io, sync::Arc};

    struct FailingWrite;
    impl WriteOperations for FailingWrite {
        fn mkdir<'a>(&'a self, _path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
            Box::pin(async { Ok(()) })
        }
        fn write_file<'a>(&'a self, _path: &'a JsString, _content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
            Box::pin(async {
                Err(io::Error::other(HandlerError::new(JsString::from_utf16(vec![
                    0xd800, 0x78, 0xdfff,
                ]))))
            })
        }
    }
    let tool = WriteTool::with_options(
        "/tmp",
        WriteToolOptions {
            operations: Some(Arc::new(FailingWrite)),
        },
    );
    let result = tool
        .execute(
            "raw-error".into(),
            parse_lossless(r#"{"path":"raw.txt","content":"x"}"#).unwrap(),
            None,
            std::sync::Arc::new(|_| Ok(())),
            None,
        )
        .await;
    assert_eq!(result.unwrap_err().message.units(), &[0xd800, 0x78, 0xdfff]);
}

#[test]
fn extension_notification_callback_receives_original_units() {
    use pi_coding_agent::core::extensions::types::{ExtensionUiContext, NotifyType};
    use pi_js_compat::js_json::JsString;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Notifications(Mutex<Vec<JsString>>);
    impl ExtensionUiContext for Notifications {
        fn notify(
            &self,
            message: &JsString,
            _kind: Option<NotifyType>,
        ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
            self.0.lock().unwrap().push(message.clone());
            Ok(())
        }
    }
    let ui = Notifications::default();
    let message = JsString::from_utf16(vec![0xd800, 0x78, 0xdfff]);
    ui.notify(&message, Some(NotifyType::Error))
        .expect("notification callback");
    assert_eq!(*ui.0.lock().unwrap(), vec![message]);
}

#[test]
fn extension_session_info_event_keeps_raw_name() {
    use pi_coding_agent::core::extensions::types::SessionInfoChangedEvent;
    let wire = r#"{"name":"\ud800\udfff\ud800"}"#;
    let event: SessionInfoChangedEvent = serde_json::from_str(wire).unwrap();
    let output = serde_json::to_string(&event).unwrap();
    assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(wire).unwrap());
}

#[test]
fn detached_outcome_tagged_decode_keeps_summary_reason_and_error() {
    use pi_coding_agent::core::detached_job_runner::DetachedJobOutcome;

    for wire in [
        r#"{"kind":"completed","summary":"\ud800"}"#,
        r#"{"kind":"aborted","reason":"\udfff"}"#,
        r#"{"kind":"failed","error":{"message":"\ud800"}}"#,
    ] {
        let result: DetachedJobOutcome = serde_json::from_str(wire).unwrap();
        let output = serde_json::to_string(&result).unwrap();
        assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(wire).unwrap());
    }
}

#[test]
fn bash_replacement_result_keeps_raw_output() {
    let result: pi_coding_agent::core::bash_executor::BashResult =
        serde_json::from_str(r#"{"output":"\ud800","exitCode":0,"cancelled":false,"truncated":false}"#).unwrap();
    assert_eq!(result.output.units(), &[0xd800]);
    let output = serde_json::to_string(&result).unwrap();
    assert_eq!(
        parse_lossless(&output).unwrap()["output"],
        parse_lossless(r#""\ud800""#).unwrap()
    );
}

#[test]
fn artifact_json_lines_and_hashes_distinguish_raw_surrogates() {
    use pi_coding_agent::core::detached_bash_runner::{append_artifact_record, hash_json, read_json_lines};

    let high = parse_lossless(r#"{"params":{"\ud800":"\udfff"}}"#).unwrap();
    let low = parse_lossless(r#"{"params":{"\udfff":"\ud800"}}"#).unwrap();
    assert_ne!(hash_json(&high), hash_json(&low));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.jsonl");
    append_artifact_record(&JsString::from(path.to_str().unwrap()), &high).unwrap();
    let first = read_json_lines(&JsString::from(path.to_str().unwrap()), 0).unwrap();
    assert_eq!(first.values, vec![high]);
    append_artifact_record(&JsString::from(path.to_str().unwrap()), &low).unwrap();
    let second = read_json_lines(&JsString::from(path.to_str().unwrap()), first.offset).unwrap();
    assert_eq!(second.values, vec![low]);
    assert!(second.offset > first.offset);
}

#[test]
fn restart_notice_stays_raw_before_process_environment_encoding() {
    use pi_coding_agent::core::self_restart::RestartEnvironment;
    use pi_coding_agent::core::self_restart::{
        ENV_SELF_RESTART_PROMPT, SelfRestartHandoff, SelfRestartRequest, build_self_restart_env,
    };
    use pi_js_compat::js_json::{JsString, encode_node_utf8};

    let request = SelfRestartRequest {
        session_file: "/tmp/restart.jsonl".into(),
        prompt: Some(JsString::from_utf16(vec![0xd800, 0x78])),
        old_pid: Some(7),
    };
    let handoff = SelfRestartHandoff::from_request(&request);
    assert_eq!(handoff.prompt.as_ref().unwrap().units(), &[0xd800, 0x78]);
    let environment = build_self_restart_env(&request, &RestartEnvironment::new(), 8);
    let notice = environment.get(ENV_SELF_RESTART_PROMPT).unwrap();
    assert_eq!(notice.units(), &[0xd800, 0x78]);
    assert_eq!(encode_node_utf8(notice), "\u{fffd}x");
}

#[test]
fn in_memory_session_name_keeps_units_after_source_normalization() {
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_js_compat::js_json::JsString;

    let mut manager = SessionManager::in_memory(None, None).unwrap();
    let name = JsString::from_utf16(vec![0x20, 0xd800, 0x0d, 0x0a, 0xdfff, 0x20]);
    manager.set_session_name(Some(&name)).unwrap();
    assert_eq!(manager.get_session_name().unwrap().units(), &[0xd800, 0x20, 0xdfff]);
    manager.set_session_name(None).unwrap();
    assert_eq!(manager.get_session_name(), None);
    assert!(manager.has_session_name_state());
}

#[test]
fn question_parser_and_answer_results_keep_raw_units_and_metadata() {
    use pi_coding_agent::core::tools::ask_questions::{format_answers, parse_params, question_result};
    let params = parse_lossless(r#"{"blocking":true,"questions":[{"question":"\ud800","options":[{"label":"\udfff"},{"label":"x"}]}],"metadata":{"\ud800":"\udfff"}}"#).unwrap();
    let parsed = parse_params(&params).unwrap();
    assert_eq!(parsed.questions[0].question.units(), &[0xd800]);
    let answers = parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap();
    let text = format_answers(&answers);
    let result = question_result(&params, text, answers.clone(), false);
    assert_eq!(result.details.as_ref().unwrap()["answers"], answers);
    assert_eq!(result.details.as_ref().unwrap()["metadata"], params["metadata"]);
}

#[test]
fn tool_truncation_keeps_units_until_the_source_utf8_suffix_boundary() {
    use pi_coding_agent::core::tools::truncate::{TruncationOptions, truncate_line, truncate_tail};
    use pi_js_compat::js_json::JsString;
    let text = JsString::from_utf16(vec![0xd800, 0x0a, 0xdfff]);
    let tail = truncate_tail(
        &text,
        TruncationOptions {
            max_lines: 1.0,
            max_bytes: 10.0,
        },
    );
    assert_eq!(tail.content.units(), &[0xdfff]);
    let pair = JsString::from_utf16(vec![0xd83d, 0xde00]);
    let line = truncate_line(&pair, Some(1.0));
    assert_eq!(line.text.units()[0], 0xd83d);
    let suffix = truncate_tail(
        JsString::from_utf16(vec![0x78, 0xdfff]),
        TruncationOptions {
            max_lines: 2.0,
            max_bytes: 3.0,
        },
    );
    assert_eq!(suffix.content.units(), &[0xfffd]);
}

#[test]
fn permission_decisions_retain_updated_input_and_raw_bash_rule_text() {
    use pi_coding_agent::core::permissions::mcp_permission_prompt::{
        PermissionPromptDecision, parse_permission_prompt_decision,
    };
    use pi_coding_agent::core::rule_store::build_permission_rule_content;
    use pi_js_compat::js_json::JsString;
    let response = parse_lossless(r#"{"behavior":"allow","updatedInput":{"command":"\ud800"}}"#).unwrap();
    let Some(PermissionPromptDecision::Allow {
        updated_input: Some(input),
        ..
    }) = parse_permission_prompt_decision(&response)
    else {
        panic!("valid allow response");
    };
    assert_eq!(
        build_permission_rule_content("bash", &input),
        JsString::from_utf16(vec![0xd800])
    );
}

#[test]
fn prompt_template_arguments_retain_surrogates_and_quote_boundaries() {
    use pi_coding_agent::core::prompt_templates::{parse_command_args, substitute_args};
    use pi_js_compat::js_json::JsString;
    let input = JsString::from_utf16(vec![0x22, 0xd800, 0x20, 0xdfff, 0x22, 0x20, 0x78]);
    let args = parse_command_args(&input);
    assert_eq!(
        args,
        vec![JsString::from_utf16(vec![0xd800, 0x20, 0xdfff]), JsString::from("x")]
    );
    let expanded = substitute_args(&JsString::from("[$1][$@]"), &args);
    assert_eq!(
        expanded.units(),
        &[
            0x5b, 0xd800, 0x20, 0xdfff, 0x5d, 0x5b, 0xd800, 0x20, 0xdfff, 0x20, 0x78, 0x5d
        ]
    );
}

#[test]
fn session_jsonl_retains_message_summary_and_custom_units_on_reload() {
    for record in [
        r#"{"type":"message","id":"m","parentId":null,"timestamp":"2026-10-21T00:00:00Z","message":{"role":"toolResult","toolCallId":"c","toolName":"probe","content":[{"type":"text","text":"\ud800"}],"details":{"\udfff":"\ud800"},"isError":false,"timestamp":1}}"#,
        r#"{"type":"custom","id":"c","parentId":null,"timestamp":"2026-10-21T00:00:00Z","customType":"probe","data":{"\ud800":"\udfff"}}"#,
        r#"{"type":"custom","id":"null","parentId":null,"timestamp":"2026-10-21T00:00:00Z","customType":"probe","data":null}"#,
        r#"{"type":"custom","id":"absent","parentId":null,"timestamp":"2026-10-21T00:00:00Z","customType":"probe"}"#,
        r#"{"type":"compaction","id":"s","parentId":null,"timestamp":"2026-10-21T00:00:00Z","summary":"\ud800","firstKeptEntryId":"m","tokensBefore":4,"details":{"path":"\udfff"}}"#,
    ] {
        let header =
            r#"{"type":"session","version":3,"id":"lossless","timestamp":"2026-10-21T00:00:00Z","cwd":"/tmp"}"#;
        let parsed = parse_session_entries(format!("{header}\n{record}\n"));
        let entry = parsed
            .iter()
            .find_map(|entry| match entry {
                FileEntry::Entry(entry) => Some(entry),
                _ => None,
            })
            .expect("valid record must not be discarded");
        // Loaded records keep the raw object authority; typed views are projections.
        if entry.entry_type() == "message" {
            let view = entry.as_message().expect("typed message view");
            let AgentMessage::ToolResult(result) = view.message.as_ref() else {
                panic!("toolResult record must project to ToolResult");
            };
            assert!(matches!(&result.content[0], TextOrImageContent::Text(text) if text.text.units() == [0xd800]));
        }
        if entry.entry_type() == "compaction" {
            assert_eq!(
                entry.as_compaction().expect("typed compaction view").summary.units(),
                [0xd800]
            );
        }
        let wire = serialize_session_entry(entry).unwrap();
        assert_eq!(parse_lossless(&wire).unwrap(), parse_lossless(record).unwrap());
        let reloaded: SessionEntry = serde_json::from_str(&wire).unwrap();
        assert_eq!(&reloaded, entry);
        if let Some(message) = reloaded.as_message() {
            assert!(matches!(message.message.as_ref(), AgentMessage::ToolResult(_)));
        }
    }
}
