//! Behavioral port of the supervisor approval-reviewer, service, client, and request-loop tests.

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_ai::request::BoxFuture;
use pi_ai::types::{AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, ToolCall, Usage};
use pi_coding_agent::core::session_control_db::{
    PostSupervisorRequestInput, SupervisorRequest, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    claim_next_supervisor_request, get_control_db_path, post_supervisor_request, read_supervisor_request,
};
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager};
use pi_coding_agent::supervisor::approval_reviewer::review_tool_call_with_supervisor;
use pi_coding_agent::supervisor::main::{
    SupervisorSession, block_supervisor_file_access, process_supervisor_request, read_current_supervisor_response,
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

#[tokio::test]
async fn allows_approved_calls_and_blocks_rejected_calls() {
    let approve = Arc::new(|| {
        Box::pin(async {
            SupervisorResponse::Approve {
                reason: "bounded".to_owned(),
            }
        }) as BoxFuture<'static, SupervisorResponse>
    });
    assert!(review_tool_call_with_supervisor(approve, None, false).await.is_none());

    let reject = Arc::new(|| {
        Box::pin(async {
            SupervisorResponse::Reject {
                reason: "unsafe".to_owned(),
            }
        }) as BoxFuture<'static, SupervisorResponse>
    });
    let blocked = review_tool_call_with_supervisor(reject, None, false).await.unwrap();
    assert_eq!(blocked.block, Some(true));
    assert_eq!(blocked.reason.as_deref(), Some("unsafe"));
}

#[tokio::test]
async fn escalates_supervisor_errors_to_the_human_reviewer() {
    let error = Arc::new(|| {
        Box::pin(async {
            SupervisorResponse::Error {
                reason: "service unavailable".to_owned(),
            }
        }) as BoxFuture<'static, SupervisorResponse>
    });
    let asked = Arc::new(Mutex::new(Vec::new()));
    let recorded = asked.clone();
    let ask_human = Arc::new(move |reason: String| {
        recorded.lock().unwrap().push(reason);
        Box::pin(async { None }) as BoxFuture<'static, Option<pi_agent_core::harness::types::BeforeToolCallResult>>
    });
    assert!(
        review_tool_call_with_supervisor(error, Some(ask_human), false)
            .await
            .is_none()
    );
    assert_eq!(*asked.lock().unwrap(), ["service unavailable"]);
}

#[tokio::test]
async fn escalates_rejection_only_for_the_ask_preset() {
    let reject = Arc::new(|| {
        Box::pin(async {
            SupervisorResponse::Reject {
                reason: "high risk".to_owned(),
            }
        }) as BoxFuture<'static, SupervisorResponse>
    });
    let ask_human = Arc::new(|_reason: String| {
        Box::pin(async {
            Some(pi_agent_core::harness::types::BeforeToolCallResult {
                block: Some(true),
                reason: Some("user denied".to_owned()),
                terminate: None,
            })
        }) as BoxFuture<'static, Option<pi_agent_core::harness::types::BeforeToolCallResult>>
    });
    let blocked = review_tool_call_with_supervisor(reject, Some(ask_human), true)
        .await
        .unwrap();
    assert_eq!(blocked.reason.as_deref(), Some("user denied"));
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
    assert!(prompt.contains("preserve every requirement and completion criterion in currentObjective"));
    assert!(prompt.contains("add proposedObjective without narrowing existing scope"));
    assert!(prompt.contains("Use kind set with a non-empty reason and objective"));

    let prompt = build_supervisor_prompt(&request(
        SupervisorRequestKind::GoalIdleReview,
        json!({"objective": "finish"}),
    ));
    assert!(prompt.contains("resident peer unblocker and policy engine"));
    assert!(prompt.contains("Treat payload.objective as the authoritative full goal"));
    assert!(prompt.contains("a completed subtask must not replace broader scope"));
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
    fn abort(&self) -> BoxFuture<'static, ()> {
        Box::pin(async {})
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
    process_supervisor_request(&control_db_path, request, session).await;
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
    process_supervisor_request(&control_db_path, goal_request, goal_session).await;
    let stored = read_supervisor_request(&control_db_path, goal_id).unwrap().unwrap();
    assert_eq!(
        stored.response,
        Some(SupervisorResponse::Error {
            reason: "Supervisor model returned no assistant text for current request".to_owned()
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
