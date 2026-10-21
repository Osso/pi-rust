//! Behavioral port of the supervisor service, client, and request-loop tests
//! (approval-reviewer cases live in `approval_reviewer.rs`).

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_ai::request::BoxFuture;
use pi_ai::types::{AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ToolCall, Usage};
use pi_coding_agent::core::session_control_db::{
    PostSupervisorRequestInput, SupervisorRequest, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    claim_next_supervisor_request, get_control_db_path, post_supervisor_request, read_supervisor_request,
};
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager};
use pi_coding_agent::supervisor::main::{
    SUPERVISOR_EXCLUDED_TOOL_NAMES, SUPERVISOR_TOOL_NAMES, SupervisorSession, block_supervisor_file_access,
    create_supervisor_settings_manager, process_supervisor_request, read_current_supervisor_response,
    validate_supervisor_extension_load,
};
use pi_coding_agent::supervisor::service::{
    RunSupervisorRequestInput, RunSupervisorRequestOutcome, build_supervisor_prompt, parse_supervisor_response,
    run_supervisor_request,
};
use serde_json::{Map, Value, json};
use tempfile::TempDir;

fn request(kind: SupervisorRequestKind, payload: Value) -> SupervisorRequest {
    SupervisorRequest {
        id: 1,
        sender_session_id: "main".to_owned(),
        project_id: "pi".to_owned(),
        kind,
        payload: payload.as_object().cloned().unwrap_or_else(Map::new),
        deadline_at: "2026-07-14T12:00:30.000Z".to_owned(),
        status: SupervisorRequestStatus::Claimed,
        created_at: "2026-07-14T12:00:00.000Z".to_owned(),
        claimed_at: Some("2026-07-14T12:00:00.000Z".to_owned()),
        claim_token: Some("runtime".to_owned()),
        completed_at: None,
        response: None,
    }
}

fn assistant(content: Vec<AssistantContent>, stop_reason: StopReason) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason,
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: 1,
    }))
}

fn text_assistant(text: &str, stop_reason: StopReason) -> AgentMessage {
    assistant(
        vec![AssistantContent::Text(TextContent::new(text.to_owned()))],
        stop_reason,
    )
}

fn tool_call_assistant(name: &str, arguments: Value, stop_reason: StopReason) -> AgentMessage {
    assistant(
        vec![AssistantContent::ToolCall(ToolCall::new(
            "call-1".to_owned(),
            name.to_owned(),
            arguments.as_object().cloned().unwrap_or_default(),
        ))],
        stop_reason,
    )
}

#[test]
fn builds_bounded_prompts_for_each_request_kind() {
    let prompt = build_supervisor_prompt(&request(
        SupervisorRequestKind::ApprovalReview,
        json!({"toolName": "read"}),
    ));
    assert!(prompt.contains("memory/supervisor/pi.md"));
    assert!(prompt.contains("\"toolName\": \"read\""));
    assert!(prompt.contains("Do not request or reconstruct historical session transcripts"));
    assert!(prompt.contains("Use kind approve or reject with a non-empty reason."));

    let prompt = build_supervisor_prompt(&request(
        SupervisorRequestKind::GoalSetReview,
        json!({"currentObjective": "ship the feature", "proposedObjective": "write one test"}),
    ));
    assert!(prompt.contains("preserve every grounded requirement, exclusion, and completion criterion"));
    assert!(prompt.contains("Only an explicit user instruction may reset or narrow that parent"));
    assert!(prompt.contains("Use kind set with a non-empty reason and objective"));

    let prompt = build_supervisor_prompt(&request(
        SupervisorRequestKind::GoalIdleReview,
        json!({"objective": "finish"}),
    ));
    assert!(prompt.contains("resident peer unblocker and policy engine"));
    assert!(prompt.contains("Treat payload.objective and any current claims as claims about the active goal"));
    assert!(prompt.contains("A child-slice completion that lacks that proof must return continue"));
    assert!(prompt.contains("instructions exactly \"Continue working toward the active goal.\""));
    assert!(prompt.contains("Use pause only when progress requires user action"));
    assert!(prompt.contains("Call supervisor_response exactly once as the final action"));

    let prompt = build_supervisor_prompt(&request(
        SupervisorRequestKind::SupervisorAdvisory,
        json!({"question": "Is anything missing?"}),
    ));
    assert!(prompt.contains("Use kind advisory with a non-empty answer"));
    assert!(prompt.contains("advisory only"));
}

#[test]
fn parses_responses_per_request_kind() {
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::ApprovalReview,
            &Value::String(r#"{"kind":"approve","reason":"bounded"}"#.to_owned())
        ),
        Some(SupervisorResponse::Approve {
            reason: "bounded".to_owned()
        })
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::ApprovalReview,
            &json!({"kind": "reject", "reason": "unsafe"})
        ),
        Some(SupervisorResponse::Reject {
            reason: "unsafe".to_owned()
        })
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::GoalIdleReview,
            &json!({"kind": "wait", "reason": "reviewer is running"})
        ),
        Some(SupervisorResponse::Wait {
            reason: "reviewer is running".to_owned()
        })
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::GoalIdleReview,
            &json!({"kind": "continue", "reason": "recovered", "instructions": "Continue the goal."})
        ),
        Some(SupervisorResponse::Continue {
            instructions: "Continue the goal.".to_owned(),
            reason: "recovered".to_owned()
        })
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::GoalIdleReview,
            &json!({"kind": "continue", "reason": "recovered", "instructions": "  "})
        ),
        None
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::ApprovalReview,
            &json!({"kind": "complete", "reason": "done"})
        ),
        None
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::GoalSetReview,
            &json!({"kind": "continue", "reason": "wrong kind"})
        ),
        None
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::SupervisorAdvisory,
            &json!({"kind": "advisory", "answer": "Nothing is missing."})
        ),
        Some(SupervisorResponse::Advisory {
            answer: "Nothing is missing.".to_owned()
        })
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::SupervisorAdvisory,
            &json!({"kind": "continue", "reason": "not advisory", "instructions": "Do work"})
        ),
        None
    );
    assert_eq!(
        parse_supervisor_response(
            SupervisorRequestKind::GoalCompletionReview,
            &json!({"content": [{"type": "text", "text": "{\"kind\":\"complete\",\"reason\":\"done\"}"}]})
        ),
        Some(SupervisorResponse::Complete {
            reason: "done".to_owned()
        })
    );
}

fn fixture() -> (TempDir, String) {
    let temp = TempDir::new().unwrap();
    let control_db_path = get_control_db_path(temp.path());
    (temp, control_db_path)
}

fn post_request(control_db_path: &str, kind: SupervisorRequestKind) -> i64 {
    post_supervisor_request(
        control_db_path,
        &PostSupervisorRequestInput {
            sender_session_id: "main".to_owned(),
            project_id: "pi".to_owned(),
            kind,
            payload: json!({"toolName": "read"}).as_object().cloned().unwrap_or_default(),
            deadline_at: pi_coding_agent::utils::iso_time::format_iso_millis(now_millis() + 30_000),
        },
    )
    .unwrap()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[tokio::test]
async fn completes_a_request_with_the_parsed_response() {
    let (_temp, control_db_path) = fixture();
    let request_id = post_request(&control_db_path, SupervisorRequestKind::ApprovalReview);
    let request = claim_next_supervisor_request(&control_db_path, "runtime")
        .unwrap()
        .unwrap();
    assert_eq!(request.id, request_id);

    let evaluate = Arc::new(|_prompt: String, _signal: tokio_util::sync::CancellationToken| {
        Box::pin(async { Ok(json!({"kind": "approve", "reason": "bounded"})) })
            as BoxFuture<'static, Result<Value, String>>
    });
    let outcome = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: control_db_path.clone(),
        evaluate,
        request,
        poll_interval_ms: Some(1),
    })
    .await
    .unwrap();
    assert_eq!(outcome, RunSupervisorRequestOutcome::Completed);

    let stored = read_supervisor_request(&control_db_path, request_id).unwrap().unwrap();
    assert_eq!(stored.status, SupervisorRequestStatus::Completed);
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Approve {
            reason: "bounded".to_owned()
        })
    );
}

#[tokio::test]
async fn cancelled_requests_abort_evaluation_without_completing_or_requeueing() {
    use pi_coding_agent::core::session_control_db::cancel_supervisor_request;
    let (_temp, path) = fixture();
    let id = post_request(&path, SupervisorRequestKind::GoalIdleReview);
    let request = claim_next_supervisor_request(&path, "runtime").unwrap().unwrap();
    let cancel_path = path.clone();
    let evaluate = Arc::new(move |_prompt: String, signal: tokio_util::sync::CancellationToken| {
        let path = cancel_path.clone();
        Box::pin(async move {
            assert!(cancel_supervisor_request(&path, id, "main", "user cancelled").unwrap());
            tokio::select! {
                () = signal.cancelled() => Err("cancelled".to_owned()),
                () = tokio::time::sleep(std::time::Duration::from_millis(50)) => Ok(json!({"kind":"complete","reason":"late"})),
            }
        }) as BoxFuture<'static, Result<Value, String>>
    });
    let outcome = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: path.clone(),
        evaluate,
        request,
        poll_interval_ms: Some(1),
    })
    .await;
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(format!("{:?}", outcome.unwrap()), "Cancelled");
    let stored = read_supervisor_request(&path, id).unwrap().unwrap();
    assert_eq!(stored.status, SupervisorRequestStatus::Cancelled);
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Error {
            reason: "user cancelled".into()
        })
    );
}

#[tokio::test]
async fn evaluator_errors_propagate_without_completing_the_request() {
    let (_temp, path) = fixture();
    let id = post_request(&path, SupervisorRequestKind::ApprovalReview);
    let request = claim_next_supervisor_request(&path, "runtime").unwrap().unwrap();
    let outcome = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: path.clone(),
        request,
        poll_interval_ms: Some(1),
        evaluate: Arc::new(|_, _| Box::pin(async { Err("provider failure".into()) })),
    })
    .await;
    assert!(outcome.is_err());
    assert_eq!(
        read_supervisor_request(&path, id).unwrap().unwrap().status,
        SupervisorRequestStatus::Claimed
    );
}

#[tokio::test]
async fn expired_requests_complete_with_the_deadline_error() {
    let (_temp, control_db_path) = fixture();
    let request_id = post_request(&control_db_path, SupervisorRequestKind::GoalIdleReview);
    let mut request = claim_next_supervisor_request(&control_db_path, "runtime")
        .unwrap()
        .unwrap();
    request.deadline_at = pi_coding_agent::utils::iso_time::format_iso_millis(now_millis() - 1);

    let evaluate = Arc::new(|_prompt: String, _signal: tokio_util::sync::CancellationToken| {
        Box::pin(async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok(json!({"kind": "complete", "reason": "late"}))
        }) as BoxFuture<'static, Result<Value, String>>
    });
    let outcome = run_supervisor_request(RunSupervisorRequestInput {
        control_db_path: control_db_path.clone(),
        evaluate,
        request,
        poll_interval_ms: Some(1),
    })
    .await
    .unwrap();
    assert_eq!(outcome, RunSupervisorRequestOutcome::Completed);

    let stored = read_supervisor_request(&control_db_path, request_id).unwrap().unwrap();
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Error {
            reason: "Supervisor request deadline expired".to_owned()
        })
    );
}

#[test]
fn blocks_file_access_outside_the_kb_and_symlink_escapes() {
    let temp = TempDir::new().unwrap();
    let kb = temp.path().join("kb");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&kb).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("outside.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(&outside, kb.join("escape")).unwrap();

    let kb_text = kb.display().to_string();
    assert!(
        block_supervisor_file_access(
            &kb_text,
            "read",
            &json!({"path": outside.join("outside.txt").display().to_string()})
        )
        .is_some()
    );
    assert!(
        block_supervisor_file_access(
            &kb_text,
            "read",
            &json!({"path": kb.join("escape/outside.txt").display().to_string()})
        )
        .is_some()
    );
    assert!(
        block_supervisor_file_access(
            &kb_text,
            "read",
            &json!({"path": kb.join("memory/supervisor/pi.md").display().to_string()})
        )
        .is_none()
    );
    assert!(
        block_supervisor_file_access(
            &kb_text,
            "bash",
            &json!({"path": outside.join("outside.txt").display().to_string()})
        )
        .is_none()
    );
}

#[tokio::test]
async fn cancelling_during_compaction_aborts_the_session_before_any_prompt() {
    struct CompactSession {
        entered: Arc<tokio::sync::Notify>,
        finished: Arc<tokio::sync::Notify>,
        aborts: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl SupervisorSession for CompactSession {
        fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
            self.aborts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.finished.notify_one();
            Box::pin(async { Ok(()) })
        }
        fn compact(&self, _: &str) -> Option<BoxFuture<'static, Result<(), String>>> {
            let entered = self.entered.clone();
            let finished = self.finished.clone();
            Some(Box::pin(async move {
                entered.notify_one();
                finished.notified().await;
                Ok(())
            }))
        }
        fn context_percent(&self) -> Option<f64> {
            Some(100.0)
        }
        fn prompt(&self, _: String) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Err("must not prompt after cancellation".into()) })
        }
        fn get_branch(&self) -> Vec<SessionEntry> {
            Vec::new()
        }
        fn get_leaf_id(&self) -> Option<String> {
            None
        }
    }
    let (_temp, path) = fixture();
    let id = post_request(&path, SupervisorRequestKind::GoalIdleReview);
    let request = claim_next_supervisor_request(&path, "owner").unwrap().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let aborts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let session = Arc::new(CompactSession {
        entered: entered.clone(),
        finished: Arc::new(tokio::sync::Notify::new()),
        aborts: aborts.clone(),
    });
    let cancel_path = path.clone();
    let cancellation = tokio::spawn(async move {
        entered.notified().await;
        assert!(
            pi_coding_agent::core::session_control_db::cancel_supervisor_request(
                &cancel_path,
                id,
                "main",
                "cancel compaction"
            )
            .unwrap()
        );
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        process_supervisor_request(&path, request, session),
    )
    .await
    .expect("compaction cancellation must settle")
    .unwrap();
    cancellation.await.unwrap();
    assert_eq!(aborts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        read_supervisor_request(&path, id).unwrap().unwrap().status,
        SupervisorRequestStatus::Cancelled
    );
}

#[tokio::test]
async fn console_prompt_failures_propagate_from_the_request_loop() {
    use pi_coding_agent::supervisor::main::{
        RunSupervisorRequestLoopInput, SupervisorConsolePromptQueue, run_supervisor_request_loop,
    };
    use pi_coding_agent::supervisor::request_wake::SupervisorRequestWakeServer;
    struct FailingSession(tokio_util::sync::CancellationToken);
    impl SupervisorSession for FailingSession {
        fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
        fn prompt(&self, _: String) -> BoxFuture<'static, Result<(), String>> {
            self.0.cancel();
            Box::pin(async { Err("console failure".into()) })
        }
        fn get_branch(&self) -> Vec<SessionEntry> {
            Vec::new()
        }
        fn get_leaf_id(&self) -> Option<String> {
            None
        }
    }
    let (_temp, path) = fixture();
    let signal = tokio_util::sync::CancellationToken::new();
    let mut queue = SupervisorConsolePromptQueue::new();
    queue.enqueue("prompt".into(), "console".into());
    let result = run_supervisor_request_loop(RunSupervisorRequestLoopInput {
        claim_token: "owner".into(),
        console_prompts: Arc::new(Mutex::new(queue)),
        control_db_path: path.clone(),
        session: Arc::new(FailingSession(signal.clone())),
        signal,
        wake_server: Arc::new(SupervisorRequestWakeServer::new(&path)),
        claim_next_request: None,
        process_request: None,
    })
    .await;
    assert_eq!(result.unwrap_err().to_string(), "console failure");
}

#[test]
fn supervisor_snapshot_advertises_stable_ready_process_identity() {
    use pi_coding_agent::supervisor::main::create_supervisor_console_snapshot;
    let a = create_supervisor_console_snapshot("/fixture", 1, None, Vec::new());
    let b = create_supervisor_console_snapshot("/fixture", 2, None, Vec::new());
    let a = serde_json::to_value(a).unwrap();
    let b = serde_json::to_value(b).unwrap();
    assert_eq!(a["identity"]["version"], pi_coding_agent::config::version());
    assert_eq!(a["identity"]["pid"], std::process::id());
    assert_eq!(a["identity"]["ready"], true);
    assert_eq!(a["identity"]["instanceId"], b["identity"]["instanceId"]);
    assert_eq!(a["identity"]["managedBy"], "external");
}

#[test]
fn supervisor_settings_and_tool_allowlists_match_the_source() {
    use pi_coding_agent::core::permissions::policy::ApprovalPolicy;
    use pi_coding_agent::core::permissions::presets::SandboxProfileName;

    let settings = create_supervisor_settings_manager();
    assert_eq!(
        settings.get_explicit_sandbox_profile(),
        Some(SandboxProfileName::FullAccess)
    );
    assert_eq!(settings.get_approval_policy(), ApprovalPolicy::AutoApprove);
    assert_eq!(SUPERVISOR_TOOL_NAMES, ["read", "edit", "write"]);
    for excluded in [
        "ask_architect",
        "attach_session_agent",
        "bash",
        "channel_post",
        "contact_parent",
        "loop",
        "manage_goal",
        "pyrun_eval",
        "restart_self",
        "resume_session",
        "send_agent_message",
        "spawn_agent",
    ] {
        assert!(SUPERVISOR_EXCLUDED_TOOL_NAMES.contains(&excluded), "{excluded}");
    }
}

#[test]
fn extension_load_diagnostics_are_rejected_before_startup() {
    assert_eq!(
        validate_supervisor_extension_load(&[("<openai-remote-compact>".to_owned(), "factory failed".to_owned())]),
        Err("Supervisor extension load failed: <openai-remote-compact>: factory failed".to_owned())
    );
    assert_eq!(validate_supervisor_extension_load(&[]), Ok(()));
}

struct SharedSession {
    manager: Arc<Mutex<SessionManager>>,
    on_prompt: Box<dyn Fn(&mut SessionManager) + Send + Sync>,
}

impl SupervisorSession for SharedSession {
    fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }

    fn prompt(&self, _content: String) -> BoxFuture<'static, Result<(), String>> {
        let manager = self.manager.clone();
        let action = &self.on_prompt;
        // Apply synchronously before returning the future so callers observe the append.
        action(&mut manager.lock().unwrap());
        Box::pin(async { Ok(()) })
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        self.manager.lock().unwrap().get_branch(None)
    }

    fn get_leaf_id(&self) -> Option<String> {
        self.manager.lock().unwrap().get_leaf_id().map(str::to_owned)
    }
}

#[tokio::test]
async fn process_request_does_not_reuse_a_prior_assistant_response() {
    let (temp, control_db_path) = fixture();
    let request_id = post_request(&control_db_path, SupervisorRequestKind::ApprovalReview);
    let request = claim_next_supervisor_request(&control_db_path, "runtime")
        .unwrap()
        .unwrap();
    let manager = Arc::new(Mutex::new(
        SessionManager::create(
            &temp.path().display().to_string(),
            Some(&temp.path().display().to_string()),
            None,
        )
        .unwrap(),
    ));
    let session = Arc::new(SharedSession {
        manager: Arc::clone(&manager),
        on_prompt: Box::new(|manager| {
            if manager.get_entries().is_empty() {
                manager
                    .append_message(text_assistant(
                        r#"{"kind":"approve","reason":"prior approval"}"#,
                        StopReason::Stop,
                    ))
                    .unwrap();
            }
        }),
    });
    process_supervisor_request(&control_db_path, request, session)
        .await
        .unwrap();
    let stored = read_supervisor_request(&control_db_path, request_id).unwrap().unwrap();
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Approve {
            reason: "prior approval".to_owned()
        })
    );

    let goal_id = post_request(&control_db_path, SupervisorRequestKind::GoalIdleReview);
    let goal_request = claim_next_supervisor_request(&control_db_path, "runtime")
        .unwrap()
        .unwrap();
    let goal_session = Arc::new(SharedSession {
        manager: Arc::clone(&manager),
        on_prompt: Box::new(|_manager| {}),
    });
    process_supervisor_request(&control_db_path, goal_request, goal_session)
        .await
        .unwrap();
    let stored = read_supervisor_request(&control_db_path, goal_id).unwrap().unwrap();
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Error {
            reason: "Supervisor model returned no assistant text for current request".to_owned()
        })
    );
}

#[tokio::test]
async fn non_generic_automatic_instructions_are_suppressed_without_a_valid_veto_decision() {
    let (_temp, path) = fixture();
    let id = post_request(&path, SupervisorRequestKind::GoalIdleReview);
    let request = claim_next_supervisor_request(&path, "runtime").unwrap().unwrap();
    let manager = Arc::new(Mutex::new(SessionManager::in_memory(None, None).unwrap()));
    let session = Arc::new(SharedSession {
        manager,
        on_prompt: Box::new(|manager| {
            manager
                .append_message(tool_call_assistant(
                    "supervisor_response",
                    json!({"kind":"continue","reason":"missing proof","instructions":"Run command X"}),
                    StopReason::ToolUse,
                ))
                .unwrap();
        }),
    });
    process_supervisor_request(&path, request, session).await.unwrap();
    assert_eq!(
        read_supervisor_request(&path, id).unwrap().unwrap().response,
        Some(SupervisorResponse::Continue {
            reason: "missing proof".into(),
            instructions: "Continue working toward the active goal.".into()
        })
    );
}

#[test]
fn structured_tool_calls_are_read_without_assistant_text() {
    let (_temp, control_db_path) = fixture();
    let _ = control_db_path;
    let mut manager = SessionManager::in_memory(None, None).unwrap();
    let previous_leaf = manager.get_leaf_id().map(str::to_owned);
    manager
        .append_message(tool_call_assistant(
            "supervisor_response",
            json!({"kind": "complete", "reason": "structured proof"}),
            StopReason::ToolUse,
        ))
        .unwrap();
    assert_eq!(
        read_current_supervisor_response(&manager.get_branch(None), previous_leaf.as_deref()).unwrap(),
        json!({"kind": "complete", "reason": "structured proof"})
    );
}

#[test]
fn end_turn_tool_use_falls_back_to_the_last_stopped_assistant_message() {
    let mut manager = SessionManager::in_memory(None, None).unwrap();
    let previous_leaf = manager.get_leaf_id().map(str::to_owned);
    manager
        .append_message(text_assistant(
            r#"{"kind":"complete","reason":"final"}"#,
            StopReason::Stop,
        ))
        .unwrap();
    manager
        .append_message(tool_call_assistant("end_turn", json!({}), StopReason::ToolUse))
        .unwrap();
    assert_eq!(
        read_current_supervisor_response(&manager.get_branch(None), previous_leaf.as_deref()).unwrap(),
        Value::String(r#"{"kind":"complete","reason":"final"}"#.to_owned())
    );
}

#[test]
fn assistant_text_with_a_structured_response_is_rejected() {
    let mut manager = SessionManager::in_memory(None, None).unwrap();
    let previous_leaf = manager.get_leaf_id().map(str::to_owned);
    manager
        .append_message(assistant(
            vec![
                AssistantContent::Text(TextContent::new("extra".to_owned())),
                AssistantContent::ToolCall(ToolCall::new(
                    "call-1".to_owned(),
                    "supervisor_response".to_owned(),
                    json!({"kind": "complete", "reason": "x"}).as_object().cloned().unwrap(),
                )),
            ],
            StopReason::ToolUse,
        ))
        .unwrap();
    assert_eq!(
        read_current_supervisor_response(&manager.get_branch(None), previous_leaf.as_deref()).unwrap_err(),
        "Supervisor model emitted assistant text with structured response"
    );
}

async fn faux_codex_runtime() -> pi_coding_agent::core::model_runtime::ModelRuntime {
    use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("openai-codex".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "gpt-6.1-sol".to_owned(),
            name: Some("Sol".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(200_000.0),
            max_tokens: Some(8192.0),
        }]),
        ..Default::default()
    });
    faux.set_responses(vec![
        pi_ai::providers::faux::faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Text("ACCEPT".into()),
            pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
        )
        .into(),
    ]);
    let runtime = pi_coding_agent::core::model_runtime::ModelRuntime::create(
        pi_coding_agent::core::model_runtime::CreateModelRuntimeOptions {
            credentials: Some(Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
                serde_json::from_value::<Map<String, Value>>(
                    json!({"openai-codex":{"type":"api_key","key":"faux-key"}}),
                )
                .unwrap(),
            ))),
            models_path: Some(None),
            refresh_on_create: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("runtime");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    runtime
        .get_available(None, Default::default())
        .await
        .expect("fake provider availability");
    runtime
}

#[tokio::test]
async fn resource_loader_registers_only_the_frozen_kb_gate() {
    use pi_coding_agent::core::extensions::types::{ToolCallEvent, ToolName};
    use pi_coding_agent::core::model_registry::ModelRegistry;
    use pi_coding_agent::core::resource_loader::ResourceLoader;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::supervisor::main::create_supervisor_resource_loader;

    let temp = tempfile::tempdir().unwrap();
    let kb = temp.path().join("kb");
    let agent = temp.path().join("agent");
    std::fs::create_dir_all(kb.join("notes")).unwrap();
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(kb.join("notes").join("inside.md"), "inside").unwrap();
    std::fs::write(temp.path().join("outside.md"), "outside").unwrap();
    let settings = Arc::new(Mutex::new(create_supervisor_settings_manager()));
    let loader = create_supervisor_resource_loader(&agent.display().to_string(), &kb.display().to_string(), settings)
        .await
        .expect("loader");
    let extensions = loader.get_extensions();
    assert!(extensions.errors.is_empty());
    assert_eq!(extensions.extensions.len(), 1);
    assert_eq!(extensions.extensions[0].read().tool_gates.len(), 1);

    let runtime = faux_codex_runtime().await;
    let runner = pi_coding_agent::core::extensions::ExtensionRunner::new(
        extensions.extensions.clone(),
        extensions.runtime.clone(),
        kb.display().to_string(),
        Arc::new(Mutex::new(
            SessionManager::in_memory(Some(&kb.display().to_string()), None).unwrap(),
        )),
        Arc::new(ModelRegistry::new(runtime)),
        None,
    );
    let context = runner.create_context();
    let gate = Arc::clone(&extensions.extensions[0].read().tool_gates[0]);
    let event = |path: &str| ToolCallEvent {
        tool_name: ToolName::Read,
        tool_call_id: "call".to_owned(),
        bypass_permissions: false,
        input: serde_json::Map::from_iter([("path".to_owned(), serde_json::json!(path))]),
    };
    let blocked = gate
        .call(
            event(&temp.path().join("outside.md").display().to_string()),
            context.clone(),
        )
        .await
        .unwrap()
        .expect("block result");
    assert_eq!(blocked.block, Some(true));
    assert_eq!(
        blocked.reason.as_deref(),
        Some("Pi Supervisor may access files only inside the configured KB directory.")
    );
    let allowed = gate
        .call(
            event(&kb.join("notes").join("inside.md").display().to_string()),
            context,
        )
        .await
        .unwrap();
    assert!(allowed.is_none());
}

#[tokio::test]
async fn instruction_evaluator_uses_only_fake_provider_and_classifies_its_exact_token() {
    use pi_coding_agent::supervisor::main::{
        create_supervisor_instruction_evaluator_with_runtime, create_supervisor_instruction_reviewer,
    };
    let runtime = faux_codex_runtime().await;
    let evaluator = create_supervisor_instruction_evaluator_with_runtime(runtime).unwrap();
    let reviewer = create_supervisor_instruction_reviewer(evaluator);
    let decision = reviewer(
        "Evidence lacks a completion proof".into(),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        decision,
        Some(pi_coding_agent::supervisor::service::SupervisorInstructionReviewDecision::Accept)
    );
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    assert_eq!(reviewer("Do not evaluate".into(), token).await.unwrap(), None);
}

#[tokio::test]
async fn supervisor_session_uses_the_sol_model_and_supervisor_tool_set() {
    use pi_coding_agent::core::resource_loader::StaticResourceLoader;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::supervisor::main::{CreateSupervisorSessionOptions, create_supervisor_session};

    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().display().to_string();
    let runtime = faux_codex_runtime().await;
    let session_manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
    let result = create_supervisor_session(CreateSupervisorSessionOptions {
        agent_dir: cwd.clone(),
        kb_dir: cwd.clone(),
        model_runtime: runtime,
        resource_loader: StaticResourceLoader::new(),
        session_manager,
        settings_manager: Arc::new(Mutex::new(create_supervisor_settings_manager())),
    })
    .await
    .expect("session");
    assert_eq!(result.session.model().provider, "openai-codex");
    assert_eq!(result.session.model().id, "gpt-6.1-sol");
    assert_eq!(
        result.session.thinking_level(),
        pi_agent_core::harness::types::ThinkingLevel::Low
    );
    assert_eq!(
        result.session.get_active_tool_names(),
        vec!["read", "edit", "write", "supervisor_response", "end_turn"]
    );
    assert!(result.session.get_tool_definition("supervisor_response").is_some());
    assert!(result.session.get_tool_definition("bash").is_none());
}
