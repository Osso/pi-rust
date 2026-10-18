use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::session::{
    EntryBody, EntryOrder, EntryQuery, EntryType, FactItem, ForkOptions, IdGenerator, InMemorySessionRepo,
    InMemorySessionStorage, OperationIntent, OperationKind, OptionalValue, RecordBody, RecordQuery, RecordType,
    Session, SessionCreateOptions, SessionErrorCode, SessionMetadata, SessionMutation, SessionStopReason,
};
use pi_ai::types::{AssistantMessage, StopReason, Usage, UserMessage, UserRole};
use serde_json::json;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(
            pi_ai::types::TextContent::new(text),
        )]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn assistant_message(text: &str) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1);
    message.content = vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
        text,
    ))];
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

fn usage(multiplier: f64) -> Usage {
    Usage {
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

fn message_body(id: &str, message: AgentMessage) -> EntryBody {
    EntryBody::Message {
        id: id.to_owned(),
        message,
        terminate: None,
    }
}

fn operation_started(id: &str, lane: &str, kind: OperationKind) -> RecordBody {
    let intent = match kind {
        OperationKind::Run => OperationIntent::Run {
            original_prompt: Vec::new(),
            initial_messages: Vec::new(),
            system_prompt_override: None,
            resume_data: None,
        },
        OperationKind::Compaction => OperationIntent::Compaction {
            custom_instructions: None,
            result_entry_id: format!("{id}-result"),
        },
        OperationKind::Navigation => OperationIntent::Navigation {
            target_id: None,
            summarize: false,
            custom_instructions: None,
            label: None,
            summary_entry_id: None,
        },
    };
    RecordBody::OperationStarted {
        id: id.to_owned(),
        lane: lane.to_owned(),
        source_leaf_id: None,
        intent,
    }
}

fn memory_session(id: &str) -> Session<SessionMetadata> {
    Session::new(Arc::new(InMemorySessionStorage::new(SessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        parent_session_id: None,
    })))
}

fn ids(entries: &[pi_agent_core::harness::session::Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.id().to_owned()).collect()
}

async fn expect_code<F, T>(future: F, code: SessionErrorCode)
where
    F: std::future::Future<Output = Result<T, pi_agent_core::harness::session::SessionError>>,
{
    match future.await {
        Ok(_) => panic!("operation must fail"),
        Err(error) => assert_eq!(error.code(), code, "message: {}", error.message()),
    }
}

#[tokio::test]
async fn assigns_parents_and_one_sequence_across_every_mutation() {
    let session = memory_session("session");
    let root = session
        .append_entry(message_body("root", user_message("root")), "main")
        .await
        .unwrap();
    session.create_lane("thread", Some(root.id())).await.unwrap();
    let child = session
        .append_entry(
            EntryBody::Custom {
                id: "child".to_owned(),
                custom_type: "note".to_owned(),
                data: json!({"value": 1}).into(),
            },
            "thread",
        )
        .await
        .unwrap();
    let record = session
        .append_record(operation_started("run", "thread", OperationKind::Run))
        .await
        .unwrap();
    session.set_name(Some("Example")).await.unwrap();
    session.set_label(root.id(), Some("checkpoint")).await.unwrap();
    session.move_lane("main", Some(child.id())).await.unwrap();

    assert_eq!(root.parent_id, None);
    assert_eq!(root.seq, 1);
    assert_eq!(child.parent_id.as_deref(), Some("root"));
    assert_eq!(child.seq, 3);
    assert_eq!(record.seq, 4);
    assert_eq!(
        session
            .get_log(Default::default())
            .await
            .unwrap()
            .iter()
            .map(|item| item.seq())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        session.get_lanes().await.unwrap(),
        vec![
            pi_agent_core::harness::session::LanePointer {
                lane: "main".to_owned(),
                leaf_id: Some("child".to_owned())
            },
            pi_agent_core::harness::session::LanePointer {
                lane: "thread".to_owned(),
                leaf_id: Some("child".to_owned())
            },
        ]
    );
}

#[tokio::test]
async fn commits_records_and_lane_moves_as_separate_mutations() {
    let session = memory_session("session");
    let root = session
        .append_entry(message_body("root", user_message("root")), "main")
        .await
        .unwrap();
    let finished = session
        .append_record(RecordBody::OperationFinished {
            id: "finish".to_owned(),
            lane: "main".to_owned(),
            run_id: "run".to_owned(),
            outcome: pi_agent_core::harness::session::OperationOutcome::Completed,
            error: None,
        })
        .await
        .unwrap();
    assert_eq!(finished.seq, 2);
    session.move_lane("main", None).await.unwrap();
    assert_eq!(
        session
            .get_log(Default::default())
            .await
            .unwrap()
            .iter()
            .map(|item| item.seq())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    expect_code(session.move_lane("main", Some("missing")), SessionErrorCode::NotFound).await;
    assert_eq!(session.find_records(RecordQuery::default()).await.unwrap().len(), 1);
    assert_eq!(root.id(), "root");
}

#[tokio::test]
async fn rejects_duplicate_ids_without_changing_state() {
    let session = memory_session("session");
    session
        .append_entry(message_body("shared", user_message("root")), "main")
        .await
        .unwrap();
    expect_code(
        session.append_record(operation_started("shared", "main", OperationKind::Run)),
        SessionErrorCode::AlreadyExists,
    )
    .await;
    session
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await
        .unwrap();
    expect_code(
        session.append_entry(
            EntryBody::Custom {
                id: "run".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing,
            },
            "main",
        ),
        SessionErrorCode::AlreadyExists,
    )
    .await;
    assert_eq!(
        session
            .get_log(Default::default())
            .await
            .unwrap()
            .iter()
            .map(|item| item.seq())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[tokio::test]
async fn supports_bounded_filtered_and_cursor_based_queries() {
    let session = memory_session("session");
    session
        .append_entry(message_body("root", user_message("root")), "main")
        .await
        .unwrap();
    session
        .append_entry(
            EntryBody::Custom {
                id: "old-note".to_owned(),
                custom_type: "note".to_owned(),
                data: json!(1).into(),
            },
            "main",
        )
        .await
        .unwrap();
    session
        .append_entry(
            EntryBody::Compaction {
                id: "compact".to_owned(),
                summary: "summary".to_owned(),
                tokens_before: 10.0,
                retained_tail: Vec::new(),
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: None,
            },
            "main",
        )
        .await
        .unwrap();
    session
        .append_entry(
            EntryBody::Custom {
                id: "new-note".to_owned(),
                custom_type: "note".to_owned(),
                data: json!(2).into(),
            },
            "main",
        )
        .await
        .unwrap();
    session
        .append_entry(message_body("tail", assistant_message("tail")), "main")
        .await
        .unwrap();

    assert_eq!(
        ids(&session.find_entries(EntryQuery::default()).await.unwrap()),
        vec!["tail", "new-note", "compact", "old-note", "root"]
    );
    assert_eq!(
        ids(&session
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                cursor: Some(pi_agent_core::harness::session::EntryCursor { after_seq: 2 }),
                limit: Some(2),
                ..EntryQuery::default()
            })
            .await
            .unwrap()),
        vec!["compact", "new-note"]
    );
    assert_eq!(
        ids(&session
            .find_entries(EntryQuery {
                custom_type: Some("note".to_owned()),
                ..EntryQuery::default()
            })
            .await
            .unwrap()),
        vec!["new-note", "old-note"]
    );
    assert_eq!(
        ids(&session
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    custom_type: Some("note".to_owned()),
                    limit: Some(1),
                    ..EntryQuery::default()
                },
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some("tail".to_owned()),
                    ..Default::default()
                },
            })
            .await
            .unwrap()),
        vec!["new-note"]
    );
    assert_eq!(
        ids(&session
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    entry_type: Some(EntryType::Message),
                    ..EntryQuery::default()
                },
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some("tail".to_owned()),
                    stop_at_type: Some(EntryType::Compaction),
                    stop_at_id: None,
                },
            })
            .await
            .unwrap()),
        vec!["tail"]
    );
    assert_eq!(
        ids(&session
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    entry_type: Some(EntryType::Custom),
                    ..EntryQuery::default()
                },
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some("tail".to_owned()),
                    stop_at_id: Some("tail".to_owned()),
                    stop_at_type: None,
                },
            })
            .await
            .unwrap()),
        Vec::<String>::new()
    );
    assert_eq!(
        ids(&session
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                },
                bounds: pi_agent_core::harness::session::BranchBounds {
                    start: Some("tail".to_owned()),
                    stop_at_type: Some(EntryType::Custom),
                    stop_at_id: None,
                },
            })
            .await
            .unwrap()),
        vec!["root", "old-note"]
    );
    expect_code(
        session.find_entries(EntryQuery {
            limit: Some(0),
            ..EntryQuery::default()
        }),
        SessionErrorCode::InvalidQuery,
    )
    .await;
    expect_code(
        session.find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
            entry: EntryQuery::default(),
            bounds: pi_agent_core::harness::session::BranchBounds {
                start: Some("missing".to_owned()),
                ..Default::default()
            },
        }),
        SessionErrorCode::NotFound,
    )
    .await;
}

#[tokio::test]
async fn filters_records_and_tracks_open_operations() {
    let session = memory_session("session");
    session
        .append_record(operation_started("run-1", "main", OperationKind::Run))
        .await
        .unwrap();
    session
        .append_record(RecordBody::StepAttempt {
            id: "attempt-1".to_owned(),
            lane: "main".to_owned(),
            run_id: "run-1".to_owned(),
            step: pi_agent_core::harness::session::StepKind::Assistant,
            attempt: 1.0,
            result_entry_id: "assistant-1".to_owned(),
            compaction_reason: None,
        })
        .await
        .unwrap();
    session.create_lane("thread", None).await.unwrap();
    session
        .append_record(operation_started("run-2", "thread", OperationKind::Run))
        .await
        .unwrap();
    session
        .append_record(RecordBody::StepAttempt {
            id: "attempt-2".to_owned(),
            lane: "thread".to_owned(),
            run_id: "run-2".to_owned(),
            step: pi_agent_core::harness::session::StepKind::Assistant,
            attempt: 1.0,
            result_entry_id: "assistant-2".to_owned(),
            compaction_reason: None,
        })
        .await
        .unwrap();

    let record_ids = |records: Vec<pi_agent_core::harness::session::LaneRecord>| {
        records.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>()
    };
    assert_eq!(
        record_ids(
            session
                .find_records(RecordQuery {
                    lane: Some("thread".to_owned()),
                    ..RecordQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["attempt-2", "run-2"]
    );
    assert_eq!(
        record_ids(
            session
                .find_records(RecordQuery {
                    record_type: Some(RecordType::StepAttempt),
                    order: Some(EntryOrder::OldestFirst),
                    ..RecordQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["attempt-1", "attempt-2"]
    );
    assert_eq!(
        record_ids(
            session
                .find_records(RecordQuery {
                    run_id: Some("run-1".to_owned()),
                    after_seq: Some(1),
                    ..RecordQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["attempt-1"]
    );
    assert_eq!(
        record_ids(
            session
                .find_records(RecordQuery {
                    limit: Some(1),
                    ..RecordQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["attempt-2"]
    );
    assert_eq!(
        record_ids(
            session
                .find_records(RecordQuery {
                    record_type: Some(RecordType::OperationStarted),
                    operation_kind: Some(OperationKind::Run),
                    ..RecordQuery::default()
                })
                .await
                .unwrap()
        ),
        vec!["run-2", "run-1"]
    );
    expect_code(
        session.find_records(RecordQuery {
            operation_kind: Some(OperationKind::Run),
            ..RecordQuery::default()
        }),
        SessionErrorCode::InvalidQuery,
    )
    .await;

    assert_eq!(
        record_ids(session.find_open_operations("main", None).await.unwrap()),
        vec!["run-1"]
    );
    expect_code(
        session.append_record(operation_started("second", "main", OperationKind::Run)),
        SessionErrorCode::Storage,
    )
    .await;
    session
        .append_record(RecordBody::OperationFinished {
            id: "finish-first".to_owned(),
            lane: "main".to_owned(),
            run_id: "run-1".to_owned(),
            outcome: pi_agent_core::harness::session::OperationOutcome::Completed,
            error: None,
        })
        .await
        .unwrap();
    assert!(session.find_open_operations("main", None).await.unwrap().is_empty());
}

#[tokio::test]
async fn keeps_latest_facts_and_computes_ledger_statistics_across_lanes() {
    let session = memory_session("session");
    let assistant = assistant_message("answer");
    session
        .append_entry(message_body("user", user_message("question")), "main")
        .await
        .unwrap();
    session
        .append_entry(message_body("assistant", assistant), "main")
        .await
        .unwrap();
    session
        .append_record(RecordBody::Usage {
            id: "assistant-usage".to_owned(),
            lane: "main".to_owned(),
            cause: pi_agent_core::harness::session::UsageCause::Assistant,
            run_id: Some("run".to_owned()),
            entry_id: Some("assistant".to_owned()),
            attempt: Some(1.0),
            stop_reason: Some(SessionStopReason::Stop),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: Usage {
                input: 10.0,
                output: 5.0,
                cache_read: 3.0,
                cache_write: 2.0,
                cache_write_1h: None,
                reasoning: None,
                total_tokens: 20.0,
                cost: pi_ai::types::UsageCost {
                    input: 1.0,
                    output: 2.0,
                    cache_read: 3.0,
                    cache_write: 4.0,
                    total: 10.0,
                },
            },
        })
        .await
        .unwrap();
    session.create_lane("thread", Some("assistant")).await.unwrap();
    session
        .append_record(RecordBody::Usage {
            id: "correction".to_owned(),
            lane: "thread".to_owned(),
            cause: pi_agent_core::harness::session::UsageCause::Adjustment,
            run_id: None,
            entry_id: None,
            attempt: None,
            stop_reason: None,
            tool_call_id: None,
            details: json!({"reason": "provider correction"}).into(),
            usage: Usage {
                input: -2.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                cache_write_1h: None,
                reasoning: None,
                total_tokens: -2.0,
                cost: pi_ai::types::UsageCost {
                    input: -0.5,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                    total: -0.5,
                },
            },
        })
        .await
        .unwrap();
    session.set_name(Some("First")).await.unwrap();
    session.set_name(Some("Second")).await.unwrap();
    session.set_label("user", Some("keep")).await.unwrap();
    session.set_label("user", None).await.unwrap();
    expect_code(
        session.set_label("missing", Some("checkpoint")),
        SessionErrorCode::NotFound,
    )
    .await;

    assert_eq!(session.get_name().await.unwrap().as_deref(), Some("Second"));
    assert_eq!(session.get_label("user").await.unwrap(), None);
    let stats = session.get_stats().await.unwrap();
    assert_eq!(stats.message_count, 2);
    assert_eq!(stats.cached_tokens, 3.0);
    assert_eq!(stats.uncached_tokens, 10.0);
    assert_eq!(stats.total_tokens, 18.0);
    assert_eq!(stats.cost_total, 9.5);
}

#[tokio::test]
async fn validates_lane_lifecycle_and_binds_views_without_caching_leaves() {
    let session = memory_session("session");
    expect_code(session.create_lane("main", None), SessionErrorCode::AlreadyExists).await;
    expect_code(
        session.create_lane("thread", Some("missing")),
        SessionErrorCode::NotFound,
    )
    .await;
    expect_code(session.move_lane("missing", None), SessionErrorCode::InvalidLane).await;

    let root = session.append_message(user_message("root")).await.unwrap();
    session.create_lane("thread", Some(&root)).await.unwrap();
    let thread = session.view("thread");
    let main_child = session.append_message(user_message("main")).await.unwrap();
    let thread_child = thread.append_message(user_message("thread")).await.unwrap();
    assert_eq!(
        session.get_leaf_id().await.unwrap().as_deref(),
        Some(main_child.as_str())
    );
    assert_eq!(
        thread.get_leaf_id().await.unwrap().as_deref(),
        Some(thread_child.as_str())
    );
    assert_eq!(
        ids(&session
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                },
                ..Default::default()
            })
            .await
            .unwrap()),
        vec![root.clone(), main_child]
    );
    assert_eq!(
        ids(&thread
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery {
                entry: EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                },
                ..Default::default()
            })
            .await
            .unwrap()),
        vec![root, thread_child]
    );
    let empty = memory_session("empty");
    assert!(
        empty
            .find_entries_on_branch(pi_agent_core::harness::session::BranchQuery::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn persists_tool_result_termination_and_rejects_non_finite_payloads() {
    let session = memory_session("session");
    let entry = session
        .append_entry(
            EntryBody::Message {
                id: "tool-result".to_owned(),
                message: AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
                    role: pi_ai::types::ToolResultRole::Tag,
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "example".to_owned(),
                    content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                        "done",
                    ))],
                    details: None,
                    usage: None,
                    added_tool_names: None,
                    is_error: false,
                    timestamp: 1,
                }),
                terminate: Some(true),
            },
            "main",
        )
        .await
        .unwrap();
    let stored = session.get_entry(entry.id()).await.unwrap().unwrap();
    assert!(matches!(
        stored.body,
        EntryBody::Message {
            terminate: Some(true),
            ..
        }
    ));

    let mut invalid = usage(1.0);
    invalid.input = f64::NAN;
    expect_code(
        session.append_entry(
            EntryBody::BranchSummary {
                id: "invalid".to_owned(),
                from_id: "tool-result".to_owned(),
                summary: "s".to_owned(),
                details: OptionalValue::Missing,
                usage: Some(invalid),
            },
            "main",
        ),
        SessionErrorCode::InvalidPayload,
    )
    .await;
}

#[tokio::test]
async fn uses_one_injectable_id_generator_across_lane_views() {
    struct Counting(AtomicUsize);
    impl IdGenerator for Counting {
        fn next(&self) -> String {
            format!("generated-{}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }
    let session = Session::with_id_generator(
        Arc::new(InMemorySessionStorage::new(SessionMetadata {
            id: "session".to_owned(),
            created_at: 1,
            parent_session_id: None,
        })),
        Arc::new(Counting(AtomicUsize::new(0))),
    );
    let main_id = session.append_custom_entry("note", None).await.unwrap();
    session.create_lane("thread", Some(&main_id)).await.unwrap();
    let thread_id = session.view("thread").append_custom_entry("note", None).await.unwrap();
    assert_eq!(main_id, "generated-1");
    assert_eq!(thread_id, "generated-2");
}

#[tokio::test]
async fn forks_branches_and_trees_with_facts_and_no_records() {
    let repo = InMemorySessionRepo::new();
    let source = repo
        .create(SessionCreateOptions {
            id: Some("source".to_owned()),
            parent_session_id: None,
        })
        .await
        .unwrap();
    let root = source.append_message(user_message("root")).await.unwrap();
    let shared = source.append_message(assistant_message("shared")).await.unwrap();
    source.create_lane("thread", Some(&shared)).await.unwrap();
    let thread_child = source
        .view("thread")
        .append_message(user_message("thread"))
        .await
        .unwrap();
    let main_child = source.append_message(user_message("main")).await.unwrap();
    source.set_name(Some("Source")).await.unwrap();
    source.set_label(&shared, Some("copied")).await.unwrap();
    source.set_label(&thread_child, Some("excluded")).await.unwrap();
    source
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await
        .unwrap();
    let source_metadata = source.get_metadata().await.unwrap();

    let fork = repo
        .fork(
            &source_metadata,
            ForkOptions::BranchAt {
                entry_id: main_child.clone(),
            },
            SessionCreateOptions {
                id: Some("branch-fork".to_owned()),
                parent_session_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        ids(&fork
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..EntryQuery::default()
            })
            .await
            .unwrap()),
        vec![root.clone(), shared.clone(), main_child.clone()]
    );
    assert_eq!(fork.get_name().await.unwrap().as_deref(), Some("Source"));
    assert_eq!(fork.get_label(&shared).await.unwrap().as_deref(), Some("copied"));
    assert_eq!(fork.get_label(&thread_child).await.unwrap(), None);
    assert!(fork.find_records(RecordQuery::default()).await.unwrap().is_empty());
    assert_eq!(fork.get_stats().await.unwrap().message_count, 3);
    assert_eq!(
        fork.get_metadata().await.unwrap().parent_session_id.as_deref(),
        Some("source")
    );

    let tree = repo
        .fork(
            &source_metadata,
            ForkOptions::Tree,
            SessionCreateOptions {
                id: Some("tree-fork".to_owned()),
                parent_session_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        ids(&tree
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..EntryQuery::default()
            })
            .await
            .unwrap()),
        vec![root.clone(), shared.clone(), thread_child.clone(), main_child.clone()]
    );
    assert_eq!(
        tree.get_label(&thread_child).await.unwrap().as_deref(),
        Some("excluded")
    );

    let before = repo
        .fork(
            &source_metadata,
            ForkOptions::BranchBefore { entry_id: main_child },
            SessionCreateOptions {
                id: Some("before".to_owned()),
                parent_session_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        ids(&before
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..EntryQuery::default()
            })
            .await
            .unwrap()),
        vec![root, shared]
    );

    expect_code(
        repo.fork(
            &source_metadata,
            ForkOptions::BranchAt {
                entry_id: "missing".to_owned(),
            },
            SessionCreateOptions {
                id: Some("bad".to_owned()),
                parent_session_id: None,
            },
        ),
        SessionErrorCode::InvalidForkTarget,
    )
    .await;
}

#[tokio::test]
async fn repository_lifecycle_and_concurrent_writes() {
    let repo = InMemorySessionRepo::new();
    let session = repo
        .create(SessionCreateOptions {
            id: Some("one".to_owned()),
            parent_session_id: None,
        })
        .await
        .unwrap();
    let entry_id = session.append_message(user_message("persisted")).await.unwrap();
    let metadata = session.get_metadata().await.unwrap();
    let listed = repo.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, metadata.id);
    let opened = repo.open(&metadata).await.unwrap();
    assert_eq!(
        ids(&opened.find_entries(EntryQuery::default()).await.unwrap()),
        vec![entry_id]
    );
    expect_code(
        repo.create(SessionCreateOptions {
            id: Some("one".to_owned()),
            parent_session_id: None,
        }),
        SessionErrorCode::AlreadyExists,
    )
    .await;
    repo.delete(&metadata).await.unwrap();
    expect_code(repo.open(&metadata), SessionErrorCode::NotFound).await;
    repo.delete(&metadata).await.unwrap();

    let concurrent = repo
        .create(SessionCreateOptions {
            id: Some("concurrent".to_owned()),
            parent_session_id: None,
        })
        .await
        .unwrap();
    let root = concurrent
        .append_entry(
            EntryBody::Custom {
                id: "root".to_owned(),
                custom_type: "root".to_owned(),
                data: OptionalValue::Missing,
            },
            "main",
        )
        .await
        .unwrap();
    concurrent.create_lane("thread", Some(root.id())).await.unwrap();
    let (main1, thread1, main2, thread2) = tokio::join!(
        concurrent.append_entry(
            EntryBody::Custom {
                id: "main-1".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing
            },
            "main"
        ),
        concurrent.append_entry(
            EntryBody::Custom {
                id: "thread-1".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing
            },
            "thread"
        ),
        concurrent.append_entry(
            EntryBody::Custom {
                id: "main-2".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing
            },
            "main"
        ),
        concurrent.append_entry(
            EntryBody::Custom {
                id: "thread-2".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Missing
            },
            "thread"
        ),
    );
    let mut seqs = vec![
        main1.unwrap().seq,
        thread1.unwrap().seq,
        main2.unwrap().seq,
        thread2.unwrap().seq,
    ];
    seqs.sort_unstable();
    assert_eq!(seqs, vec![3, 4, 5, 6]);
    let log_seqs: Vec<i64> = concurrent
        .get_log(Default::default())
        .await
        .unwrap()
        .iter()
        .map(|item| item.seq())
        .collect();
    assert_eq!(log_seqs, vec![1, 2, 3, 4, 5, 6]);
}

#[tokio::test]
async fn applies_fork_mutations_through_the_state_machine() {
    let mut state = pi_agent_core::harness::session::state::SessionState::new();
    state
        .apply_mutation(&SessionMutation::Fact {
            seq: 1,
            fact: FactItem::Name {
                name: Some("Example".to_owned()),
            },
        })
        .unwrap();
    assert_eq!(state.get_name().as_deref(), Some("Example"));
}
