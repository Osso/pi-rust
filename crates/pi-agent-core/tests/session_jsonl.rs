use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::session::{
    EntryBody, EntryOrder, EntryQuery, EntryType, ForkOptions, JsonlSessionCreateOptions, JsonlSessionListOptions,
    JsonlSessionMetadata, JsonlSessionRepo, JsonlSessionRepoFileSystem, JsonlSessionRepoOptions, OptionalValue,
    RecordBody, RecordQuery, RecordType, SessionErrorCode, UsageCause,
};
use pi_agent_core::harness::types::{FileError, FileInfo};
use pi_ai::request::BoxFuture;
use pi_ai::types::{AssistantMessage, StopReason, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole};
use serde_json::json;
use tempfile::TempDir;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn usage(multiplier: f64) -> pi_ai::types::Usage {
    pi_ai::types::Usage {
        input: multiplier,
        output: multiplier * 2.0,
        cache_read: multiplier * 3.0,
        cache_write: multiplier * 4.0,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: multiplier * 10.0,
        cost: pi_ai::types::UsageCost {
            input: multiplier * 0.1,
            output: multiplier * 0.2,
            cache_read: multiplier * 0.3,
            cache_write: multiplier * 0.4,
            total: multiplier,
        },
    }
}

fn repo(root: &Path) -> JsonlSessionRepo {
    JsonlSessionRepo::with_clock(
        JsonlSessionRepoOptions {
            fs: Arc::new(pi_agent_core::harness::session::jsonl::types::NativeFileSystem::new(
                root,
            )),
            sessions_root: root.to_string_lossy().into_owned(),
        },
        Arc::new(|| 1_700_000_000_000),
    )
}

fn repo_with_fs(root: &Path, fs: Arc<dyn JsonlSessionRepoFileSystem>) -> JsonlSessionRepo {
    JsonlSessionRepo::with_clock(
        JsonlSessionRepoOptions {
            fs,
            sessions_root: root.to_string_lossy().into_owned(),
        },
        Arc::new(|| 1_700_000_000_000),
    )
}

fn create_options(id: &str, cwd: &Path) -> JsonlSessionCreateOptions {
    JsonlSessionCreateOptions {
        id: Some(id.to_owned()),
        parent_session_id: None,
        cwd: cwd.to_string_lossy().into_owned(),
        metadata: None,
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn find_session_file(root: &Path, id: &str) -> std::path::PathBuf {
    let suffix = format!("_{id}.jsonl");
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().unwrap().to_string_lossy().ends_with(&suffix) {
                return path;
            }
        }
    }
    panic!("session file for {id} not found");
}

fn file_ids(entries: &[pi_agent_core::harness::session::Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.id().to_owned()).collect()
}

async fn open_session(
    root: &Path,
    metadata: &JsonlSessionMetadata,
) -> pi_agent_core::harness::session::Session<JsonlSessionMetadata> {
    repo(root).open(metadata).await.unwrap()
}

#[tokio::test]
async fn resume_data_serializes_in_source_key_order() {
    let root = TempDir::new().unwrap();
    let session = repo(root.path())
        .create(create_options("resume-order", root.path()))
        .await
        .unwrap();
    let mut resume_data = indexmap::IndexMap::new();
    resume_data.insert("zeta".to_owned(), json!({"n": 1}));
    resume_data.insert("alpha".to_owned(), json!({"n": 2}));
    session
        .append_record(RecordBody::OperationStarted {
            id: "run".to_owned(),
            lane: "main".to_owned(),
            source_leaf_id: None,
            intent: pi_agent_core::harness::session::OperationIntent::Run {
                original_prompt: vec![user_message("prompt")],
                initial_messages: Vec::new(),
                system_prompt_override: None,
                resume_data: Some(resume_data),
            },
        })
        .await
        .unwrap();
    let raw = read(&find_session_file(root.path(), "resume-order"));
    let line = raw
        .lines()
        .find(|line| line.contains("\"resumeData\""))
        .expect("resumeData record");
    let parsed: serde_json::Value = serde_json::from_str(line).expect("record JSON");
    let keys: Vec<&String> = parsed["intent"]["resumeData"]
        .as_object()
        .expect("resumeData object")
        .keys()
        .collect();
    assert_eq!(keys, vec!["zeta", "alpha"]);
}

#[tokio::test]
async fn round_trips_every_entry_type_and_bounded_branch_queries() {
    let root = TempDir::new().unwrap();
    let session = repo(root.path())
        .create(create_options("entries", root.path()))
        .await
        .unwrap();
    let mut committed = Vec::new();
    committed.push(
        session
            .append_entry(
                EntryBody::Message {
                    id: "message".to_owned(),
                    message: user_message("question"),
                    terminate: None,
                },
                "main",
            )
            .await
            .unwrap(),
    );
    let mut assistant = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 2);
    assistant.content = vec![
        pi_ai::types::AssistantContent::Text(TextContent::new("I'll inspect it.")),
        pi_ai::types::AssistantContent::ToolCall(pi_ai::types::ToolCall {
            r#type: pi_ai::types::ToolCallTag::Tag,
            id: "call-1".to_owned(),
            name: "read".to_owned(),
            arguments: json!({"path": "README.md"}).as_object().unwrap().clone(),
            execution: None,
            thought_signature: None,
            namespace: None,
            index: None,
            partial_json: None,
        }),
    ];
    assistant.usage = usage(1.0);
    assistant.stop_reason = StopReason::ToolUse;
    committed.push(
        session
            .append_entry(
                EntryBody::Message {
                    id: "assistant-tool-call".to_owned(),
                    message: AgentMessage::Assistant(Box::new(assistant)),
                    terminate: None,
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::Message {
                    id: "tool-result".to_owned(),
                    message: AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
                        role: pi_ai::types::ToolResultRole::Tag,
                        tool_call_id: "call-1".to_owned(),
                        tool_name: "read".to_owned(),
                        content: vec![TextOrImageContent::Text(TextContent::new("contents"))],
                        details: Some(json!({"path": "README.md"})),
                        usage: Some(usage(2.0)),
                        added_tool_names: None,
                        is_error: false,
                        timestamp: 3,
                    }),
                    terminate: Some(true),
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::ModelChange {
                    id: "model".to_owned(),
                    provider: "anthropic".to_owned(),
                    model_id: "claude-sonnet-4-5".to_owned(),
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::ThinkingLevelChange {
                    id: "thinking".to_owned(),
                    thinking_level: "high".to_owned(),
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::ActiveToolsChange {
                    id: "tools".to_owned(),
                    active_tool_names: vec!["read".to_owned(), "bash".to_owned()],
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::Compaction {
                    id: "compaction".to_owned(),
                    summary: "summary".to_owned(),
                    tokens_before: 123.0,
                    retained_tail: vec![user_message("retained")],
                    duration_ms: None,
                    details: json!({"source": "test"}).into(),
                    usage: Some(usage(1.0)),
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::BranchSummary {
                    id: "branch-summary".to_owned(),
                    from_id: "message".to_owned(),
                    summary: "branch".to_owned(),
                    details: json!({"reason": "navigation"}).into(),
                    usage: Some(usage(2.0)),
                },
                "main",
            )
            .await
            .unwrap(),
    );
    committed.push(
        session
            .append_entry(
                EntryBody::Custom {
                    id: "custom".to_owned(),
                    custom_type: "note".to_owned(),
                    data: json!({"nested": {"value": 1}}).into(),
                },
                "main",
            )
            .await
            .unwrap(),
    );

    let metadata = session.get_metadata().await.unwrap();
    let restored = open_session(root.path(), &metadata).await;
    assert_eq!(
        restored
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..EntryQuery::default()
            })
            .await
            .unwrap(),
        committed
    );
    assert_eq!(
        file_ids(
            &restored
                .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                    entry: EntryQuery::default(),
                    bounds: pi_agent_core::harness::session::BranchBounds {
                        start: None,
                        stop_at_type: Some(EntryType::Compaction),
                        stop_at_id: None,
                    },
                })
                .await
                .unwrap()
        ),
        vec!["custom", "branch-summary", "compaction"]
    );
    assert_eq!(
        file_ids(
            &restored
                .find_entries(EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    cursor: Some(pi_agent_core::harness::session::EntryCursor {
                        after_seq: committed[5].seq
                    }),
                    limit: Some(2),
                    ..EntryQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["compaction", "branch-summary"]
    );
    assert_eq!(
        file_ids(
            &restored
                .find_entries(EntryQuery {
                    custom_type: Some("note".to_owned()),
                    ..EntryQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["custom"]
    );
    let stats = restored.get_stats().await.unwrap();
    assert_eq!(stats.message_count, 3);
    assert_eq!(stats.cached_tokens, 0.0);
    assert_eq!(stats.total_tokens, 0.0);

    let mut custom = restored.get_entry("custom").await.unwrap().unwrap();
    if let EntryBody::Custom { data, .. } = &mut custom.body {
        *data = json!({"nested": {"value": 99}}).into();
    }
    assert_eq!(
        restored.get_entry("custom").await.unwrap().unwrap(),
        committed.last().unwrap().clone()
    );
}

#[tokio::test]
async fn round_trips_every_record_type_and_ledger_statistics() {
    let root = TempDir::new().unwrap();
    let session = repo(root.path())
        .create(create_options("records", root.path()))
        .await
        .unwrap();
    let anchor = session.append_custom_entry("anchor", None).await.unwrap();
    let mut records = Vec::new();
    records.push(
        session
            .append_record(RecordBody::OperationStarted {
                id: "run".to_owned(),
                lane: "main".to_owned(),
                source_leaf_id: Some(anchor.clone()),
                intent: pi_agent_core::harness::session::OperationIntent::Run {
                    original_prompt: vec![user_message("prompt")],
                    initial_messages: vec![EntryBody::Message {
                        id: "initial".to_owned(),
                        message: user_message("initial"),
                        terminate: None,
                    }],
                    system_prompt_override: Some("system".to_owned()),
                    resume_data: Some(indexmap::IndexMap::from([(
                        "extension".to_owned(),
                        json!({"version": 1}),
                    )])),
                },
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::QueueEnqueued {
                id: "steer".to_owned(),
                lane: "main".to_owned(),
                queue: pi_agent_core::harness::session::QueueKind::Steer,
                run_id: Some("run".to_owned()),
                target: EntryBody::Message {
                    id: "steer-message".to_owned(),
                    message: user_message("steer"),
                    terminate: None,
                },
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::StepAttempt {
                id: "assistant-attempt".to_owned(),
                lane: "main".to_owned(),
                run_id: "run".to_owned(),
                step: pi_agent_core::harness::session::StepKind::Assistant,
                attempt: 1.0,
                result_entry_id: "assistant-result".to_owned(),
                compaction_reason: None,
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::ToolStarted {
                id: "tool".to_owned(),
                lane: "main".to_owned(),
                run_id: "run".to_owned(),
                assistant_entry_id: "assistant-result".to_owned(),
                tool_index: 0.0,
                tool_call_id: "call-1".to_owned(),
                tool_name: "read".to_owned(),
                effective_args: json!({"path": "README.md"}).as_object().unwrap().clone(),
                result_entry_id: "tool-result".to_owned(),
                replay: pi_agent_core::harness::session::ReplaySafety::Safe,
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::WriteDeferred {
                id: "deferred-write".to_owned(),
                lane: "main".to_owned(),
                run_id: "run".to_owned(),
                target: EntryBody::Custom {
                    id: "deferred-entry".to_owned(),
                    custom_type: "fact".to_owned(),
                    data: json!({"value": true}).into(),
                },
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::Usage {
                id: "assistant-usage".to_owned(),
                lane: "main".to_owned(),
                cause: UsageCause::Assistant,
                run_id: Some("run".to_owned()),
                entry_id: Some("assistant-result".to_owned()),
                attempt: Some(1.0),
                stop_reason: Some(pi_agent_core::harness::session::SessionStopReason::Stop),
                tool_call_id: None,
                details: OptionalValue::Missing,
                usage: usage(1.0),
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::Usage {
                id: "tool-usage".to_owned(),
                lane: "main".to_owned(),
                cause: UsageCause::Tool,
                run_id: Some("run".to_owned()),
                entry_id: Some("tool-result".to_owned()),
                attempt: None,
                stop_reason: None,
                tool_call_id: Some("call-1".to_owned()),
                details: OptionalValue::Missing,
                usage: usage(3.0),
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::Usage {
                id: "adjustment".to_owned(),
                lane: "main".to_owned(),
                cause: UsageCause::Adjustment,
                run_id: None,
                entry_id: None,
                attempt: None,
                stop_reason: None,
                tool_call_id: None,
                details: json!({"reason": "correction"}).into(),
                usage: usage(5.0),
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::AbortRequested {
                id: "abort".to_owned(),
                lane: "main".to_owned(),
                run_id: "run".to_owned(),
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::OperationFinished {
                id: "run-finished".to_owned(),
                lane: "main".to_owned(),
                run_id: "run".to_owned(),
                outcome: pi_agent_core::harness::session::OperationOutcome::Aborted,
                error: None,
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::QueueEnqueued {
                id: "next-run".to_owned(),
                lane: "main".to_owned(),
                queue: pi_agent_core::harness::session::QueueKind::NextRun,
                run_id: None,
                target: EntryBody::Message {
                    id: "next-message".to_owned(),
                    message: user_message("next"),
                    terminate: None,
                },
            })
            .await
            .unwrap(),
    );
    records.push(
        session
            .append_record(RecordBody::QueueCancelled {
                id: "queue-cancelled".to_owned(),
                lane: "main".to_owned(),
                run_id: None,
                entry_id: "next-message".to_owned(),
            })
            .await
            .unwrap(),
    );

    let metadata = session.get_metadata().await.unwrap();
    let restored = open_session(root.path(), &metadata).await;
    assert_eq!(
        restored
            .find_records(RecordQuery {
                order: Some(EntryOrder::OldestFirst),
                ..RecordQuery::default()
            })
            .await
            .unwrap(),
        records
    );
    assert_eq!(
        restored
            .find_records(RecordQuery {
                record_type: Some(RecordType::OperationStarted),
                operation_kind: Some(pi_agent_core::harness::session::OperationKind::Run),
                limit: Some(1),
                ..RecordQuery::default()
            })
            .await
            .unwrap()
            .iter()
            .map(|record| record.id().to_owned())
            .collect::<Vec<_>>(),
        vec!["run"]
    );
    let stats = restored.get_stats().await.unwrap();
    assert_eq!(stats.cached_tokens, 27.0);
    assert_eq!(stats.uncached_tokens, 45.0);
    assert_eq!(stats.total_tokens, 90.0);
    assert_eq!(stats.cost_total, 9.0);
    let mut started = restored
        .find_records(RecordQuery {
            record_type: Some(RecordType::OperationStarted),
            ..RecordQuery::default()
        })
        .await
        .unwrap()[0]
        .clone();
    if let RecordBody::OperationStarted {
        intent: pi_agent_core::harness::session::OperationIntent::Run { original_prompt, .. },
        ..
    } = &mut started.body
    {
        original_prompt.push(user_message("mutated"));
    }
    assert_eq!(
        restored
            .find_records(RecordQuery {
                order: Some(EntryOrder::OldestFirst),
                ..RecordQuery::default()
            })
            .await
            .unwrap(),
        records
    );
}

#[tokio::test]
async fn writes_one_line_per_mutation_and_restores_the_shared_sequence() {
    let root = TempDir::new().unwrap();
    let session = repo(root.path())
        .create(create_options("session", root.path()))
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    let entry_id = session
        .append_custom_entry("note", Some(json!({"value": 1})))
        .await
        .unwrap();
    session.create_lane("thread", Some(&entry_id)).await.unwrap();
    session
        .append_record(RecordBody::OperationStarted {
            id: "run".to_owned(),
            lane: "thread".to_owned(),
            source_leaf_id: None,
            intent: pi_agent_core::harness::session::OperationIntent::Run {
                original_prompt: Vec::new(),
                initial_messages: Vec::new(),
                system_prompt_override: None,
                resume_data: None,
            },
        })
        .await
        .unwrap();
    session.set_name(Some("Example")).await.unwrap();
    session.set_label(&entry_id, Some("checkpoint")).await.unwrap();
    session.move_lane("main", None).await.unwrap();

    let lines: Vec<serde_json::Value> = read(Path::new(&metadata.path))
        .trim_end()
        .split('\n')
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        lines
            .iter()
            .map(|line| line["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["header", "entry", "lane", "record", "fact", "fact", "lane"]
    );
    assert_eq!(
        lines[1..]
            .iter()
            .map(|line| line["seq"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );

    let reopened = open_session(root.path(), &metadata).await;
    assert_eq!(
        reopened.get_lanes().await.unwrap(),
        vec![
            pi_agent_core::harness::session::LanePointer {
                lane: "main".to_owned(),
                leaf_id: None
            },
            pi_agent_core::harness::session::LanePointer {
                lane: "thread".to_owned(),
                leaf_id: Some(entry_id.clone())
            }
        ]
    );
    assert_eq!(reopened.get_name().await.unwrap().as_deref(), Some("Example"));
    assert_eq!(
        reopened.get_label(&entry_id).await.unwrap().as_deref(),
        Some("checkpoint")
    );
    assert_eq!(
        reopened
            .append_record(RecordBody::OperationFinished {
                id: "finish".to_owned(),
                lane: "thread".to_owned(),
                run_id: "run".to_owned(),
                outcome: pi_agent_core::harness::session::OperationOutcome::Completed,
                error: None,
            })
            .await
            .unwrap()
            .seq,
        7
    );
    assert!(reopened.find_open_operations("thread", None).await.unwrap().is_empty());
}

#[tokio::test]
async fn repairs_torn_tails_and_unterminated_lines() {
    let root = TempDir::new().unwrap();
    let session = repo(root.path())
        .create(create_options("session", root.path()))
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    session
        .append_custom_entry("note", Some(json!({"value": "kept"})))
        .await
        .unwrap();
    let valid_prefix = read(Path::new(&metadata.path));

    std::fs::OpenOptions::new()
        .append(true)
        .open(&metadata.path)
        .unwrap()
        .write_all(b"{\"kind\":\"entry\"")
        .unwrap();
    let reopened = open_session(root.path(), &metadata).await;
    assert_eq!(read(Path::new(&metadata.path)), valid_prefix);
    assert_eq!(reopened.find_entries(EntryQuery::default()).await.unwrap().len(), 1);
    let appended = reopened.append_custom_entry("after-recovery", None).await.unwrap();
    assert_eq!(reopened.get_entry(&appended).await.unwrap().unwrap().seq, 2);

    let unterminated = read(Path::new(&metadata.path)).trim_end().to_owned();
    std::fs::write(&metadata.path, &unterminated).unwrap();
    let reopened = open_session(root.path(), &metadata).await;
    assert_eq!(read(Path::new(&metadata.path)), format!("{unterminated}\n"));
    assert_eq!(reopened.find_entries(EntryQuery::default()).await.unwrap().len(), 2);
}

#[tokio::test]
async fn rejects_invalid_headers_and_replay_errors_without_modifying_files() {
    let root = TempDir::new().unwrap();
    let repository = repo(root.path());
    repository.create(create_options("valid", root.path())).await.unwrap();
    let session = repository
        .create(create_options("malformed-header", root.path()))
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    let malformed = "not json\n";
    std::fs::write(&metadata.path, malformed).unwrap();
    let error = repository.open(&metadata).await.unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert_eq!(
        repository.list(JsonlSessionListOptions::default()).await.unwrap().len(),
        1
    );
    assert_eq!(read(Path::new(&metadata.path)), malformed);

    let raw = raw_session(
        root.path(),
        "missing-parent",
        &[json!({
            "kind": "entry",
            "type": "custom",
            "id": "orphan",
            "customType": "note",
            "parentId": "missing",
            "seq": 1,
            "timestamp": 1
        })],
    );
    let error = repository.open(&raw).await.unwrap_err();
    assert_eq!(
        error.message(),
        format!(
            "Invalid JSONL v4 session {}: line 2 Invalid session mutation: references missing parent missing",
            raw.path
        )
    );

    let invalid_final = raw_session(root.path(), "invalid-final", &[json!({"kind": "unknown", "seq": 1})]);
    let corrupted = read(Path::new(&invalid_final.path));
    let error = repository.open(&invalid_final).await.unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
    assert_eq!(read(Path::new(&invalid_final.path)), corrupted);
}

#[tokio::test]
async fn rejects_lane_chaining_and_sequence_errors_during_replay() {
    let root = TempDir::new().unwrap();
    let repository = repo(root.path());
    let cases = [
        (
            "non-consecutive",
            json!({"kind": "entry", "type": "custom", "id": "entry", "customType": "note", "parentId": null, "seq": 2, "timestamp": 1}),
            "non-consecutive seq",
        ),
        (
            "missing-lane",
            json!({"kind": "entry", "lane": "thread", "type": "custom", "id": "entry", "customType": "note", "parentId": null, "seq": 1, "timestamp": 1}),
            "missing lane",
        ),
        (
            "missing-lane-target",
            json!({"kind": "lane", "lane": "thread", "leafId": "missing", "seq": 1}),
            "missing lane target",
        ),
        (
            "missing-label-target",
            json!({"kind": "fact", "fact": "label", "targetId": "missing", "label": "checkpoint", "seq": 1}),
            "missing label target",
        ),
    ];
    for (id, mutation, expected) in cases {
        let metadata = raw_session(root.path(), id, &[mutation]);
        let error = repository.open(&metadata).await.unwrap_err();
        assert_eq!(error.code(), SessionErrorCode::InvalidEntry);
        assert!(
            error.message().contains(expected),
            "expected {expected} in {}",
            error.message()
        );
    }

    let session = repository
        .create(create_options("lane-chain", root.path()))
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    session.append_custom_entry("first", None).await.unwrap();
    session.append_custom_entry("second", None).await.unwrap();
    let mut lines: Vec<serde_json::Value> = read(Path::new(&metadata.path))
        .trim_end()
        .split('\n')
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    lines[2]["parentId"] = json!(null);
    let corrupted = format!(
        "{}\n",
        lines.iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n")
    );
    std::fs::write(&metadata.path, corrupted).unwrap();
    let error = repository.open(&metadata).await.unwrap_err();
    assert!(error.message().contains("does not chain to the lane leaf"));
}

#[tokio::test]
async fn imports_entries_without_lane_metadata_and_moves_lanes_explicitly() {
    let root = TempDir::new().unwrap();
    let metadata = raw_session(
        root.path(),
        "import",
        &[json!({
            "kind": "entry",
            "type": "custom",
            "id": "imported",
            "customType": "note",
            "parentId": null,
            "seq": 1,
            "timestamp": 1
        })],
    );
    let imported = repo(root.path()).open(&metadata).await.unwrap();
    assert_eq!(imported.get_leaf_id().await.unwrap(), None);
    assert_eq!(
        file_ids(&imported.find_entries(EntryQuery::default()).await.unwrap()),
        vec!["imported"]
    );

    let mut content = read(Path::new(&metadata.path));
    content.push_str(&format!(
        "{}\n",
        json!({"kind": "lane", "seq": 2, "lane": "main", "leafId": "imported"})
    ));
    std::fs::write(&metadata.path, content).unwrap();
    let moved = repo(root.path()).open(&metadata).await.unwrap();
    assert_eq!(moved.get_leaf_id().await.unwrap().as_deref(), Some("imported"));
}

#[tokio::test]
async fn forks_branches_and_trees_and_reopens_lanes_and_facts() {
    let root = TempDir::new().unwrap();
    let repository = repo(root.path());
    let source = repository.create(create_options("source", root.path())).await.unwrap();
    let root_id = source.append_custom_entry("root", None).await.unwrap();
    source.create_lane("thread", Some(&root_id)).await.unwrap();
    let main_id = source
        .append_entry(
            EntryBody::Message {
                id: "main".to_owned(),
                message: user_message("main"),
                terminate: None,
            },
            "main",
        )
        .await
        .unwrap()
        .id()
        .to_owned();
    let thread_id = source
        .append_entry(
            EntryBody::Custom {
                id: "thread".to_owned(),
                custom_type: "thread".to_owned(),
                data: OptionalValue::Missing,
            },
            "thread",
        )
        .await
        .unwrap()
        .id()
        .to_owned();
    source.set_name(Some("Source")).await.unwrap();
    source.set_label(&thread_id, Some("tip")).await.unwrap();
    let source_metadata = source.get_metadata().await.unwrap();

    let fork = repository
        .fork(&source_metadata, ForkOptions::Tree, create_options("fork", root.path()))
        .await
        .unwrap();
    let metadata = fork.get_metadata().await.unwrap();
    let imported_lanes: Vec<bool> = read(Path::new(&metadata.path))
        .trim_end()
        .split('\n')
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|line| line["kind"] == json!("entry"))
        .map(|line| line.get("lane").is_some())
        .collect();
    assert_eq!(imported_lanes, vec![false, false, false]);

    let reopened = repository.open(&metadata).await.unwrap();
    assert_eq!(
        file_ids(
            &reopened
                .find_entries(EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                })
                .await
                .unwrap()
        ),
        vec![root_id.clone(), main_id.clone(), thread_id.clone()]
    );
    assert_eq!(
        reopened.get_lanes().await.unwrap(),
        vec![
            pi_agent_core::harness::session::LanePointer {
                lane: "main".to_owned(),
                leaf_id: Some(main_id.clone())
            },
            pi_agent_core::harness::session::LanePointer {
                lane: "thread".to_owned(),
                leaf_id: Some(thread_id.clone())
            }
        ]
    );
    assert_eq!(reopened.get_name().await.unwrap().as_deref(), Some("Source"));
    assert_eq!(reopened.get_label(&thread_id).await.unwrap().as_deref(), Some("tip"));
    assert!(reopened.find_records(RecordQuery::default()).await.unwrap().is_empty());

    let branch = repository
        .fork(
            &source_metadata,
            ForkOptions::BranchBefore {
                entry_id: main_id.clone(),
            },
            create_options("branch", root.path()),
        )
        .await
        .unwrap();
    assert_eq!(
        file_ids(
            &branch
                .find_entries(EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                })
                .await
                .unwrap()
        ),
        vec![root_id.clone()]
    );

    let before_default = repository
        .fork(
            &source_metadata,
            ForkOptions::BranchDefaultBefore,
            create_options("before-default", root.path()),
        )
        .await
        .unwrap();
    assert_eq!(
        file_ids(
            &before_default
                .find_entries(EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                })
                .await
                .unwrap()
        ),
        vec![root_id]
    );

    let at_default = repository
        .fork(
            &source_metadata,
            ForkOptions::Branch,
            create_options("at-default", root.path()),
        )
        .await
        .unwrap();
    assert_eq!(
        at_default.get_leaf_id().await.unwrap().as_deref(),
        Some(main_id.as_str())
    );
}

#[tokio::test]
async fn metadata_contract_id_validation_and_listing_order() {
    let root = TempDir::new().unwrap();
    let repository = repo(root.path());
    let cwd = root.path().join("workspace").join("project");
    let session = repository
        .create(JsonlSessionCreateOptions {
            id: Some("metadata".to_owned()),
            parent_session_id: Some("parent".to_owned()),
            cwd: cwd.to_string_lossy().into_owned(),
            metadata: Some(
                json!({"owner": "agent", "nested": {"enabled": true}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        })
        .await
        .unwrap();
    let metadata = session.get_metadata().await.unwrap();
    assert_eq!(metadata.id, "metadata");
    assert_eq!(metadata.parent_session_id.as_deref(), Some("parent"));
    assert_eq!(metadata.cwd, cwd.to_string_lossy());
    assert_eq!(metadata.source_format, 4);
    assert_eq!(metadata.metadata.as_ref().unwrap()["owner"], json!("agent"));
    assert_eq!(
        repository
            .list(JsonlSessionListOptions {
                cwd: Some(cwd.to_string_lossy().into_owned())
            })
            .await
            .unwrap(),
        vec![metadata.clone()]
    );
    assert!(
        repository
            .list(JsonlSessionListOptions {
                cwd: Some(root.path().join("other").to_string_lossy().into_owned())
            })
            .await
            .unwrap()
            .is_empty()
    );

    let error = repository
        .create(create_options("../escape", root.path()))
        .await
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::InvalidPayload);

    let first = repository
        .create(create_options("shared", &root.path().join("workspaces/first")))
        .await
        .unwrap();
    let second = repository
        .create(create_options("shared", &root.path().join("workspaces/second")))
        .await
        .unwrap();
    assert_ne!(
        first.get_metadata().await.unwrap().cwd,
        second.get_metadata().await.unwrap().cwd
    );
    let listed = repository.list(JsonlSessionListOptions::default()).await.unwrap();
    assert_eq!(listed.len(), 3);
    let mut modified: Vec<f64> = listed.iter().map(|metadata| metadata.modified_at).collect();
    modified.sort_by(|left, right| right.partial_cmp(left).unwrap());
    assert_eq!(
        listed.iter().map(|metadata| metadata.modified_at).collect::<Vec<_>>(),
        modified
    );
}

#[tokio::test]
async fn delete_is_idempotent_and_open_requires_the_file() {
    let root = TempDir::new().unwrap();
    let repository = repo(root.path());
    let session = repository.create(create_options("one", root.path())).await.unwrap();
    let metadata = session.get_metadata().await.unwrap();
    repository.delete(&metadata).await.unwrap();
    assert_eq!(
        repository.open(&metadata).await.unwrap_err().code(),
        SessionErrorCode::NotFound
    );
    repository.delete(&metadata).await.unwrap();
}

struct FailingFileSystem {
    inner: pi_agent_core::harness::session::jsonl::types::NativeFileSystem,
    fail_append: AtomicBool,
    fail_rename: AtomicBool,
}

impl FailingFileSystem {
    fn new(root: &Path) -> Self {
        Self {
            inner: pi_agent_core::harness::session::jsonl::types::NativeFileSystem::new(root),
            fail_append: AtomicBool::new(false),
            fail_rename: AtomicBool::new(false),
        }
    }

    fn failure(path: &str) -> FileError {
        FileError::new(
            pi_agent_core::harness::types::FileErrorCode::Unknown,
            "injected failure",
            Some(path.to_owned()),
            None,
        )
    }
}

impl JsonlSessionRepoFileSystem for FailingFileSystem {
    fn absolute_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        self.inner.absolute_path(path)
    }

    fn join_path<'a>(&'a self, parts: &'a [String]) -> BoxFuture<'a, Result<String, FileError>> {
        self.inner.join_path(parts)
    }

    fn read_text_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        self.inner.read_text_file(path)
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
        max_lines: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>> {
        self.inner.read_text_lines(path, max_lines)
    }

    fn write_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        self.inner.write_file(path, content)
    }

    fn append_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        let fail = self.fail_append.swap(false, Ordering::SeqCst);
        let path = path.to_owned();
        Box::pin(async move {
            if fail {
                return Err(Self::failure(&path));
            }
            self.inner.append_file(&path, content).await
        })
    }

    fn rename_file<'a>(&'a self, source: &'a str, destination: &'a str) -> BoxFuture<'a, Result<(), FileError>> {
        let fail = self.fail_rename.swap(false, Ordering::SeqCst);
        let source = source.to_owned();
        let destination = destination.to_owned();
        Box::pin(async move {
            if fail {
                return Err(Self::failure(&source));
            }
            self.inner.rename_file(&source, &destination).await
        })
    }

    fn file_info<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<FileInfo, FileError>> {
        self.inner.file_info(path)
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>> {
        self.inner.list_dir(path)
    }

    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<bool, FileError>> {
        self.inner.exists(path)
    }

    fn create_dir<'a>(&'a self, path: &'a str, recursive: bool) -> BoxFuture<'a, Result<(), FileError>> {
        self.inner.create_dir(path, recursive)
    }

    fn remove<'a>(&'a self, path: &'a str, recursive: bool, force: bool) -> BoxFuture<'a, Result<(), FileError>> {
        self.inner.remove(path, recursive, force)
    }
}

#[tokio::test]
async fn append_failure_does_not_advance_state_or_poison_writes() {
    let root = TempDir::new().unwrap();
    let fs = Arc::new(FailingFileSystem::new(root.path()));
    let repository = repo_with_fs(root.path(), fs.clone());
    let session = repository
        .create(create_options("append-failure", root.path()))
        .await
        .unwrap();
    fs.fail_append.store(true, Ordering::SeqCst);
    assert_eq!(
        session.append_custom_entry("rejected", None).await.unwrap_err().code(),
        SessionErrorCode::Storage
    );
    assert!(session.get_log(Default::default()).await.unwrap().is_empty());
    let committed = session
        .append_entry(
            EntryBody::Custom {
                id: "committed".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing,
            },
            "main",
        )
        .await
        .unwrap();
    assert_eq!(committed.seq, 1);
}

#[tokio::test]
async fn fork_failures_leave_no_partial_file() {
    let root = TempDir::new().unwrap();
    let fs = Arc::new(FailingFileSystem::new(root.path()));
    let repository = repo_with_fs(root.path(), fs.clone());
    let source = repository.create(create_options("source", root.path())).await.unwrap();
    source.append_message(user_message("one")).await.unwrap();
    source.append_message(user_message("two")).await.unwrap();
    let source_metadata = source.get_metadata().await.unwrap();

    fs.fail_append.store(true, Ordering::SeqCst);
    let error = repository
        .fork(
            &source_metadata,
            ForkOptions::Branch,
            create_options("fork", root.path()),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert_eq!(
        repository.list(JsonlSessionListOptions::default()).await.unwrap().len(),
        1
    );
    let directory = Path::new(&source_metadata.path).parent().unwrap();
    assert!(
        std::fs::read_dir(directory).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );

    fs.fail_rename.store(true, Ordering::SeqCst);
    let error = repository
        .fork(
            &source_metadata,
            ForkOptions::Branch,
            create_options("fork", root.path()),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), SessionErrorCode::Storage);
    assert_eq!(
        repository.list(JsonlSessionListOptions::default()).await.unwrap().len(),
        1
    );
    assert!(
        std::fs::read_dir(directory).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
}

fn raw_session(root: &Path, id: &str, mutations: &[serde_json::Value]) -> JsonlSessionMetadata {
    let path = root.join(format!("{id}.jsonl"));
    let header = json!({"kind": "header", "version": 4, "id": id, "createdAt": 1, "cwd": root});
    let mut lines = vec![header.to_string()];
    lines.extend(mutations.iter().map(serde_json::Value::to_string));
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    JsonlSessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        cwd: root.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        modified_at: 0.0,
        source_format: 4,
        parent_session_id: None,
        legacy_parent_session_path: None,
        metadata: None,
    }
}
