//! UTF-16 mailbox content must survive both persistent and in-memory public APIs.
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::{self, JsString, JsValue};

#[test]
fn enqueue_retry_restore_retains_raw_payload_and_rejects_replacement_collision() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let wire = r#"{"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","updatedAt":"\udfff","fileRefs":[{"path":"/raw/\udfff","label":"\ud800"}],"unknown":{"\ud800":"high","�":"replacement"}}}"#;
    let input: EnqueueStoredRuntimeMailboxMessageInput =
        js_json::from_value(&js_json::parse_lossless(wire).unwrap()).unwrap();
    let id = enqueue_stored_runtime_mailbox_message(&database, &input).unwrap();
    assert_eq!(enqueue_stored_runtime_mailbox_message(&database, &input).unwrap(), id);
    let message = read_runtime_mailbox_message(&database, id).unwrap().unwrap();
    let projection = js_json::to_value(&message).unwrap();
    assert_eq!(projection.get("body"), input.message.get("body"));
    assert_eq!(projection.get("fileRefs"), input.message.get("fileRefs"));
    assert_eq!(projection.get("updatedAt"), input.message.get("updatedAt"));
    let state = read_multi_agent_state(&database, "/fixture/s").unwrap().unwrap();
    assert_eq!(state.mailbox_messages[0].get("body"), input.message.get("body"));
    struct Persistence(String);
    impl pi_coding_agent::core::multi_agent_store::SessionPersistence for Persistence {
        fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
            Some(self.0.as_str().into())
        }
        fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
            Some("/fixture/s".into())
        }
    }
    let store = pi_coding_agent::core::multi_agent_store::MultiAgentStore::from_session_manager(
        &Persistence(database.clone()),
        Default::default(),
    )
    .unwrap();
    let restored = store.list_mailbox_messages().unwrap();
    assert_eq!(
        js_json::to_value(&restored[0]).unwrap().get("body"),
        input.message.get("body")
    );
    let mut missing_update = input.clone();
    missing_update.store_ref.message_id = "without_update".into();
    let object = missing_update.message.as_object_mut().unwrap();
    object.insert("id".into(), JsValue::String("without_update".into()));
    object.shift_remove("updatedAt");
    enqueue_stored_runtime_mailbox_message(&database, &missing_update).unwrap();
    store
        .restore_from_session_manager(&Persistence(database.clone()))
        .unwrap();
    let restored = store.list_mailbox_messages().unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[1].updated_at, None);
    assert_eq!(restored[1].body, restored[0].body);
    let mut collision = input.clone();
    collision
        .message
        .as_object_mut()
        .unwrap()
        .insert("body".into(), JsValue::String("before�after".into()));
    assert!(enqueue_stored_runtime_mailbox_message(&database, &collision).is_err());
}

#[test]
fn supervisor_request_and_response_preserve_raw_content() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let payload = js_json::parse_lossless(r#"{"question":"why\ud800","\udfff":"low","�":"replacement"}"#).unwrap();
    let input = PostSupervisorRequestInput {
        sender_session_id: "from".into(),
        project_id: "p".into(),
        kind: SupervisorRequestKind::SupervisorAdvisory,
        payload: payload.as_object().unwrap().clone(),
        deadline_at: "2099-01-01T00:00:00.000Z".into(),
    };
    let id = post_supervisor_request(&database, &input).unwrap();
    let restored = read_supervisor_request(&database, id).unwrap().unwrap();
    assert_eq!(restored.payload, input.payload);
    let prompt = pi_coding_agent::supervisor::service::build_supervisor_prompt(&restored);
    let request_start = prompt.find("Request:\n").unwrap() + "Request:\n".len();
    let request_json = prompt.slice(request_start, prompt.len());
    let request = js_json::parse_lossless(request_json).unwrap();
    assert_eq!(request.get("payload"), Some(&payload));

    let response: SupervisorResponse =
        js_json::from_value(&js_json::parse_lossless(r#"{"kind":"advisory","answer":"raw\udfff"}"#).unwrap()).unwrap();
    assert_eq!(
        js_json::stringify_lossless(&js_json::to_value(&response).unwrap()),
        r#"{"answer":"raw\udfff","kind":"advisory"}"#
    );
}

#[test]
fn store_message_serialization_preserves_raw_addresses_file_refs_and_unknown_keys() {
    use pi_coding_agent::core::multi_agent_store::{AgentMailboxMessage, copy_message};
    let raw = js_json::parse_lossless(r#"{"id":"m\udfff","threadId":"t\ud800","fromAgentId":"from\ud800","toAgentId":"to\udfff","kind":"message","status":"pending","createdAt":"2026-10-21T00:00:00.000Z","updatedAt":"2026-10-21T00:00:00.000Z","body":"raw\ud800","fileRefs":[{"path":"/raw/\udfff","label":"\ud800"}],"\ud800":"high","�":"replacement"}"#).unwrap();
    let message: AgentMailboxMessage = js_json::from_value(&raw).unwrap();
    let serialized = js_json::to_value(&message).unwrap();
    assert_eq!(serialized, raw);
    let copied = js_json::to_value(&copy_message(&message).unwrap()).unwrap();
    for key in ["id", "threadId", "fromAgentId", "toAgentId", "body", "fileRefs"] {
        assert_eq!(copied.get(key), raw.get(key));
    }
    assert_eq!(message.extra.len(), 2);
}

#[test]
fn terminal_notifications_keep_content_units_and_duration() {
    use pi_coding_agent::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, agent_to_value};
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let raw = js_json::parse_lossless(r#"{"id":"agent_1","displayName":"Scout","agentType":"scout","lifecycle":"completed","revision":2,"createdAt":"","updatedAt":"","cwd":"/repo","permission":{"policy":"on-request","narrowed":true},"result":{"summary":"  raw\ud800  ","durationMs":42}}"#).unwrap();
    let mut agent: AgentSnapshot = js_json::from_value(&raw).unwrap();
    agent.created_at = now.clone().into();
    agent.updated_at = now.into();
    assert_eq!(agent_to_value(&agent).unwrap().get("result"), raw.get("result"));
    let store = MultiAgentStore::new(Default::default());
    store.publish_terminal_outbox_snapshot(&agent).unwrap();
    let messages = store.list_mailbox_messages().unwrap();
    let expected = js_json::parse_lossless(r#""Scout completed: raw\ud800. Duration: 42ms""#).unwrap();
    assert_eq!(messages[0].body.as_ref(), expected.as_string());
    agent.lifecycle = pi_coding_agent::core::multi_agent_store::AgentLifecycle::Failed;
    agent.result = None;
    agent.error = Some(pi_coding_agent::core::multi_agent_store::AgentError {
        message: js_json::parse_lossless(r#""  failed\udfff  ""#)
            .unwrap()
            .as_string()
            .unwrap()
            .clone(),
        code: None,
    });
    let directory = tempfile::tempdir().unwrap();
    let database = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    bootstrap_multi_agent_agent(&database, "/fixture/s", &agent.id, &agent_to_value(&agent).unwrap()).unwrap();
    let stored = read_multi_agent_agent(&database, "/fixture/s", &agent.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.error, agent.error);
    let failed_store = MultiAgentStore::new(Default::default());
    failed_store.publish_terminal_outbox_snapshot(&agent).unwrap();
    let failed = failed_store.list_mailbox_messages().unwrap();
    let expected = js_json::parse_lossless(r#""Scout failed: failed\udfff""#).unwrap();
    assert_eq!(failed[0].body.as_ref(), expected.as_string());
}

#[test]
fn session_name_normalization_retains_units_until_sql_text_binding() {
    use pi_js_compat::js_json::JsString;
    let input = JsString::from_utf16(vec![0xfeff, 0xd800, 13, 10, 13, 9, 98, 0xdcff, 0x3000]);
    let expected = JsString::from_utf16(vec![0xd800, 32, 9, 98, 0xdcff]);
    assert_eq!(normalize_session_name(Some(&input)), Some(expected.clone()));
    assert_eq!(normalize_session_name(Some(&JsString::from("\r\n "))), None);
    assert_eq!(
        normalize_session_name(Some(&JsString::from("\u{feff}A\r\n\rB \u{3000}"))),
        Some("A B".into())
    );
    let directory = tempfile::tempdir().unwrap();
    let database = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let metadata: WritableSessionMetadata = serde_json::from_value(serde_json::json!({
        "sessionPath":"/fixture/s","id":"s","cwd":"/repo","createdAt":now,"modifiedAt":now,
        "messageCount":0,"firstMessage":""
    }))
    .unwrap();
    write_session_metadata(&database, &metadata).unwrap();
    write_session_name(&database, "/fixture/s", Some(&input)).unwrap();
    assert_eq!(
        read_session_name_state(&database, "/fixture/s").unwrap().name,
        Some("\u{fffd} \tb\u{fffd}".into())
    );
    assert_eq!(input.units()[1], 0xd800);
}

#[test]
fn detached_control_delivery_keeps_result_error_and_cancel_reason() {
    use pi_coding_agent::core::detached_job_control::{
        DetachedJobRuntimeCommand, EnqueueDetachedJobStatusResponseInput, claim_detached_job_runtime_commands,
        enqueue_detached_job_status_response,
    };
    let directory = tempfile::tempdir().unwrap();
    let database = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let recipient = RuntimeMailboxAddress {
        session_id: "to".into(),
        agent_id: None,
    };
    let runner = RuntimeMailboxAddress {
        session_id: "runner".into(),
        agent_id: None,
    };
    register_runtime_mailbox_listener(&database, &recipient, std::process::id(), None, Default::default()).unwrap();
    let identity = DetachedJobOwnershipIdentity {
        job_id: "job".into(),
        owner: MultiAgentRuntimeOwnerIdentity {
            session_id: "owner".into(),
            agent_id: None,
        },
        process_identity: pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap(),
        output_label: "output".into(),
    };
    let result = js_json::parse_lossless(r#"{"\ud800":["\udfff",null],"�":"replacement"}"#).unwrap();
    enqueue_detached_job_status_response(&EnqueueDetachedJobStatusResponseInput {
        control_db_path: database.clone().into(),
        identity: identity.clone(),
        reply_to: recipient.clone(),
        request_id: "request".into(),
        runner_address: runner.clone(),
        session_path: "/fixture/s".into(),
        status: result.clone(),
    })
    .unwrap();
    let raw_error = js_json::parse_lossless(r#""error\ud800""#).unwrap();
    for (id, command, key) in [("cancel", "cancel", "reason"), ("error", "respond", "error")] {
        let mut body = pi_js_compat::js_json!({"command":command,"identity":identity,"requestId":"request"});
        body.as_object_mut().unwrap().insert(key.into(), raw_error.clone());
        enqueue_stored_runtime_mailbox_message(&database, &EnqueueStoredRuntimeMailboxMessageInput {
            recipient_host: None, recipient: recipient.clone(), sender: runner.clone(), kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef { message_id: id.into(), session_path: "/fixture/s".into() },
            message: pi_js_compat::js_json!({"id":id,"fromAgentId":"main","toAgentId":"main","kind":"system","status":"pending","body":js_json::stringify_lossless(&body)}),
            updated_at: None,
        }).unwrap();
    }
    let commands = claim_detached_job_runtime_commands(&JsString::from(&database), &recipient, &identity).unwrap();
    assert_eq!(commands.len(), 3);
    let DetachedJobRuntimeCommand::Response(response) = &commands[0] else {
        panic!("response");
    };
    assert_eq!(response.result.as_ref(), Some(&result));
    let DetachedJobRuntimeCommand::Cancel(cancel) = &commands[1] else {
        panic!("cancel");
    };
    assert_eq!(cancel.reason.as_ref(), raw_error.as_string());
    let DetachedJobRuntimeCommand::Response(error) = &commands[2] else {
        panic!("error");
    };
    assert_eq!(error.error.as_ref(), raw_error.as_string());
    assert_eq!(error.result, None);
}
