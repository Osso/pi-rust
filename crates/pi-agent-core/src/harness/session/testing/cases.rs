//! Assertions ported from the pinned public conformance factory, reusing the crate's
//! existing native case fixtures rather than implementing another backend.
use super::types::{ConformanceResult, SessionBackendRepository};
use crate::agent::AgentMessage;
use crate::harness::session::{
    BranchQuery, Entry, EntryBody, EntryOrder, EntryQuery, ForkOptions, LanePointer, LogItem, LogOptions,
    OperationIntent, OperationKind, OperationOutcome, OptionalValue, QueueKind, RecordBody, RecordQuery, RecordType,
    SessionCreateOptions, SessionError, SessionErrorCode, SessionMetadataValue, SessionStats, SessionStopReason,
    StepKind, UsageCause,
};
use pi_ai::types::{AssistantMessage, StopReason, Usage, UsageCost, UserMessage, UserRole};
use serde_json::{Value, json};

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

fn message_entry(id: &str, message: AgentMessage) -> EntryBody {
    EntryBody::Message {
        id: id.to_owned(),
        message,
        terminate: None,
    }
}

fn custom_entry(id: &str, custom_type: &str, data: Option<Value>) -> EntryBody {
    EntryBody::Custom {
        id: id.to_owned(),
        custom_type: custom_type.to_owned(),
        data: data.map(pi_js_compat::js_json::JsValue::from).into(),
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

fn operation_finished(id: &str, lane: &str, run_id: &str) -> RecordBody {
    RecordBody::OperationFinished {
        id: id.to_owned(),
        lane: lane.to_owned(),
        run_id: run_id.to_owned(),
        outcome: OperationOutcome::Completed,
        error: None,
    }
}

fn ids(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.id().to_owned()).collect()
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, SessionError>, code: SessionErrorCode) {
    match result {
        Ok(value) => panic!("expected {code:?}, got {value:?}"),
        Err(error) => assert_eq!(error.code(), code, "unexpected error: {error}"),
    }
}

fn log_kinds(items: &[LogItem]) -> Vec<(&'static str, i64)> {
    items
        .iter()
        .map(|item| match item {
            LogItem::Entry { seq, .. } => ("entry", *seq),
            LogItem::Record { seq, .. } => ("record", *seq),
            LogItem::Lane { seq, .. } => ("lane", *seq),
            LogItem::Fact { seq, .. } => ("fact", *seq),
        })
        .collect()
}

fn lanes(entries: &[LanePointer]) -> Vec<(String, Option<String>)> {
    entries
        .iter()
        .map(|pointer| (pointer.lane.clone(), pointer.leaf_id.clone()))
        .collect()
}

pub(super) async fn assigns_parents_and_one_sequence_across_every_mutation<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let root = session
        .append_entry(message_entry("root", user_message("root")), "main")
        .await?;
    session.create_lane("thread", Some(root.id())).await?;
    let child = session
        .append_entry(custom_entry("child", "note", Some(json!({"value": 1}))), "thread")
        .await?;
    let record = session
        .append_record(operation_started("run", "thread", OperationKind::Run))
        .await?;
    session.set_name(Some("Example")).await?;
    session.set_label(root.id(), Some("checkpoint")).await?;
    session.move_lane("main", Some(child.id())).await?;

    assert_eq!((root.parent_id.clone(), root.seq), (None, 1));
    assert_eq!((child.parent_id.clone(), child.seq), (Some("root".to_owned()), 3));
    assert_eq!(record.seq, 4);
    for timestamp in [root.timestamp, child.timestamp, record.timestamp] {
        assert!(
            (0..=9_007_199_254_740_991).contains(&timestamp),
            "storage-assigned timestamps must be safe Unix milliseconds"
        );
    }
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(
        log_kinds(&log),
        vec![
            ("entry", 1),
            ("lane", 2),
            ("entry", 3),
            ("record", 4),
            ("fact", 5),
            ("fact", 6),
            ("lane", 7),
        ]
    );
    assert_eq!(
        lanes(&session.get_lanes().await?),
        vec![
            ("main".to_owned(), Some("child".to_owned())),
            ("thread".to_owned(), Some("child".to_owned())),
        ]
    );
    Ok(())
}

pub(super) async fn commits_records_and_lane_moves_as_separate_mutations<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let root = session
        .append_entry(message_entry("root", user_message("root")), "main")
        .await?;
    let finished = session
        .append_record(operation_finished("finish", "main", "run"))
        .await?;
    assert_eq!(finished.seq, 2);
    assert_eq!(
        lanes(&session.get_lanes().await?),
        vec![("main".to_owned(), Some("root".to_owned()))]
    );
    session.move_lane("main", None).await?;
    assert_eq!(lanes(&session.get_lanes().await?), vec![("main".to_owned(), None)]);
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(
        log,
        vec![
            LogItem::Entry {
                seq: 1,
                entry: root.clone()
            },
            LogItem::Record {
                seq: 2,
                record: finished.clone()
            },
            LogItem::Lane {
                seq: 3,
                lane: "main".to_owned(),
                leaf_id: None
            },
        ]
    );

    assert_code(
        session.move_lane("main", Some("missing")).await,
        SessionErrorCode::NotFound,
    );
    assert_eq!(session.find_records(RecordQuery::default()).await?.len(), 1);
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(log.iter().map(LogItem::seq).collect::<Vec<_>>(), vec![1, 2, 3]);
    Ok(())
}

pub(super) async fn rejects_duplicate_ids_without_changing_state<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_entry(message_entry("shared", user_message("root")), "main")
        .await?;
    assert_code(
        session
            .append_record(operation_started("shared", "main", OperationKind::Run))
            .await,
        SessionErrorCode::AlreadyExists,
    );
    session
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await?;
    assert_code(
        session.append_entry(custom_entry("run", "note", None), "main").await,
        SessionErrorCode::AlreadyExists,
    );
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(log.iter().map(LogItem::seq).collect::<Vec<_>>(), vec![1, 2]);
    Ok(())
}

pub(super) async fn isolates_lanes_while_sharing_the_tree<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_entry(message_entry("root", user_message("root")), "main")
        .await?;
    session.create_lane("thread", Some("root")).await?;
    session
        .append_entry(message_entry("main-child", user_message("main")), "main")
        .await?;
    session
        .append_entry(message_entry("thread-child", user_message("thread")), "thread")
        .await?;

    assert_eq!(
        lanes(&session.get_lanes().await?),
        vec![
            ("main".to_owned(), Some("main-child".to_owned())),
            ("thread".to_owned(), Some("thread-child".to_owned())),
        ]
    );
    let main_branch = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("main-child".to_owned()),
                ..Default::default()
            },
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(ids(&main_branch), vec!["root", "main-child"]);
    let thread_branch = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("thread-child".to_owned()),
                ..Default::default()
            },
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(ids(&thread_branch), vec!["root", "thread-child"]);
    Ok(())
}

pub(super) async fn rejects_invalid_queries_before_empty_reads<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("invalid-queries".into()),
            ..Default::default()
        })
        .await?;
    session.create_lane("thread", None).await?;
    let thread = session.view("thread");

    assert_code(
        session
            .find_entries(EntryQuery {
                limit: Some(0),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_entry(EntryQuery {
                limit: Some(0),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_entries_on_branch(BranchQuery {
                entry: EntryQuery {
                    limit: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        thread
            .find_entries_on_branch(BranchQuery {
                entry: EntryQuery {
                    cursor: Some(crate::harness::session::EntryCursor { after_seq: -1 }),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        thread
            .find_entry_on_branch(BranchQuery {
                entry: EntryQuery {
                    limit: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_records(RecordQuery {
                limit: Some(0),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_records(RecordQuery {
                operation_kind: Some(OperationKind::Run),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_records(RecordQuery {
                record_type: Some(RecordType::StepAttempt),
                operation_kind: Some(OperationKind::Run),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session.find_open_operations("main", Some(0)).await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .get_log(LogOptions {
                after_seq: Some(-1),
                limit: None,
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    Ok(())
}

pub(super) async fn supports_bounded_filtered_and_cursor_based_queries<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_entry(message_entry("root", user_message("root")), "main")
        .await?;
    session
        .append_entry(custom_entry("old-note", "note", Some(json!(1))), "main")
        .await?;
    session
        .append_entry(
            EntryBody::Compaction {
                id: "compact".to_owned(),
                summary: "summary".to_owned().into(),
                tokens_before: 10.0,
                retained_tail: Vec::new(),
                duration_ms: None,
                details: OptionalValue::Missing,
                usage: None,
            },
            "main",
        )
        .await?;
    session
        .append_entry(custom_entry("new-note", "note", Some(json!(2))), "main")
        .await?;
    session
        .append_entry(message_entry("tail", assistant_message("tail")), "main")
        .await?;

    let all = session.find_entries(EntryQuery::default()).await?;
    assert_eq!(ids(&all), vec!["tail", "new-note", "compact", "old-note", "root"]);
    let window = session
        .find_entries(EntryQuery {
            order: Some(EntryOrder::OldestFirst),
            cursor: Some(crate::harness::session::EntryCursor { after_seq: 2 }),
            limit: Some(2),
            ..Default::default()
        })
        .await?;
    assert_eq!(ids(&window), vec!["compact", "new-note"]);
    let notes = session
        .find_entries(EntryQuery {
            custom_type: Some("note".to_owned()),
            ..Default::default()
        })
        .await?;
    assert_eq!(ids(&notes), vec!["new-note", "old-note"]);
    let branch_note = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("tail".to_owned()),
                ..Default::default()
            },
            entry: EntryQuery {
                custom_type: Some("note".to_owned()),
                limit: Some(1),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(ids(&branch_note), vec!["new-note"]);
    let stopped_message = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("tail".to_owned()),
                stop_at_type: Some(crate::harness::session::EntryType::Compaction),
                ..Default::default()
            },
            entry: EntryQuery {
                entry_type: Some(crate::harness::session::EntryType::Message),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(ids(&stopped_message), vec!["tail"]);
    let stopped_custom = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("tail".to_owned()),
                stop_at_id: Some("tail".to_owned()),
                ..Default::default()
            },
            entry: EntryQuery {
                entry_type: Some(crate::harness::session::EntryType::Custom),
                ..Default::default()
            },
        })
        .await?;
    assert!(stopped_custom.is_empty());
    let oldest_to_custom = session
        .find_entries_on_branch(BranchQuery {
            bounds: crate::harness::session::BranchBounds {
                start: Some("tail".to_owned()),
                stop_at_type: Some(crate::harness::session::EntryType::Custom),
                ..Default::default()
            },
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(ids(&oldest_to_custom), vec!["root", "old-note"]);
    assert_code(
        session
            .find_entries(EntryQuery {
                limit: Some(0),
                ..Default::default()
            })
            .await,
        SessionErrorCode::InvalidQuery,
    );
    assert_code(
        session
            .find_entries_on_branch(BranchQuery {
                bounds: crate::harness::session::BranchBounds {
                    start: Some("missing".to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await,
        SessionErrorCode::NotFound,
    );
    Ok(())
}

pub(super) async fn keeps_lane_names_permanent_with_their_recovery_records<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session.create_lane("thread", None).await?;
    session
        .append_record(operation_started("old-run", "thread", OperationKind::Run))
        .await?;
    session
        .append_record(RecordBody::QueueEnqueued {
            id: "old-next-run".to_owned(),
            lane: "thread".to_owned(),
            queue: QueueKind::NextRun,
            run_id: None,
            target: message_entry("queued-message", user_message("queued")),
        })
        .await?;
    let records = session
        .find_records(RecordQuery {
            lane: Some("thread".to_owned()),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        records.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["old-next-run", "old-run"]
    );
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(
        log.iter()
            .filter_map(|item| match item {
                LogItem::Record { record, .. } => Some(record.id().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec!["old-run", "old-next-run"]
    );
    assert_code(
        session.create_lane("thread", None).await,
        SessionErrorCode::AlreadyExists,
    );
    Ok(())
}

pub(super) async fn persists_queue_cancellation_without_consuming_its_target<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let enqueued = session
        .append_record(RecordBody::QueueEnqueued {
            id: "enqueue".to_owned(),
            lane: "main".to_owned(),
            queue: QueueKind::NextRun,
            run_id: None,
            target: message_entry("queued-message", user_message("queued")),
        })
        .await?;
    let cancelled = session
        .append_record(RecordBody::QueueCancelled {
            id: "cancel".to_owned(),
            lane: "main".to_owned(),
            run_id: None,
            entry_id: "queued-message".to_owned(),
        })
        .await?;
    assert_eq!(cancelled.seq, 2);
    match &cancelled.body {
        RecordBody::QueueCancelled { entry_id, run_id, .. } => {
            assert_eq!(entry_id, "queued-message");
            assert!(run_id.is_none());
        }
        other => panic!("unexpected record {other:?}"),
    }
    assert!(session.get_entry("queued-message").await?.is_none());
    let cancellations = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::QueueCancelled),
            ..Default::default()
        })
        .await?;
    assert_eq!(cancellations, vec![cancelled.clone()]);
    let log = session.get_log(LogOptions::default()).await?;
    assert_eq!(
        log,
        vec![
            LogItem::Record {
                seq: enqueued.seq,
                record: enqueued
            },
            LogItem::Record {
                seq: cancelled.seq,
                record: cancelled
            },
        ]
    );
    Ok(())
}

pub(super) async fn filters_records_by_lane_type_run_sequence_and_order<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_record(operation_started("run-1", "main", OperationKind::Run))
        .await?;
    session
        .append_record(RecordBody::StepAttempt {
            id: "attempt-1".to_owned(),
            lane: "main".to_owned(),
            run_id: "run-1".to_owned(),
            step: StepKind::Assistant,
            attempt: 1.0,
            result_entry_id: "assistant-1".to_owned(),
            compaction_reason: None,
        })
        .await?;
    session.create_lane("thread", None).await?;
    session
        .append_record(operation_started("run-2", "thread", OperationKind::Run))
        .await?;
    session
        .append_record(RecordBody::StepAttempt {
            id: "attempt-2".to_owned(),
            lane: "thread".to_owned(),
            run_id: "run-2".to_owned(),
            step: StepKind::Assistant,
            attempt: 1.0,
            result_entry_id: "assistant-2".to_owned(),
            compaction_reason: None,
        })
        .await?;

    let thread = session
        .find_records(RecordQuery {
            lane: Some("thread".to_owned()),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        thread.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["attempt-2", "run-2"]
    );
    let attempts = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::StepAttempt),
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        attempts.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["attempt-1", "attempt-2"]
    );
    let run_1 = session
        .find_records(RecordQuery {
            run_id: Some("run-1".to_owned()),
            after_seq: Some(1),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        run_1.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["attempt-1"]
    );
    let limited = session
        .find_records(RecordQuery {
            limit: Some(1),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        limited.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["attempt-2"]
    );
    Ok(())
}

pub(super) async fn filters_operation_starts_by_operation_kind<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_record(operation_started("run-old", "main", OperationKind::Run))
        .await?;
    session
        .append_record(operation_finished("run-old-finished", "main", "run-old"))
        .await?;
    session
        .append_record(operation_started("compaction", "main", OperationKind::Compaction))
        .await?;
    session
        .append_record(operation_finished("compaction-finished", "main", "compaction"))
        .await?;
    session
        .append_record(operation_started("navigation", "main", OperationKind::Navigation))
        .await?;
    session
        .append_record(operation_finished("navigation-finished", "main", "navigation"))
        .await?;
    session
        .append_record(operation_started("run-new", "main", OperationKind::Run))
        .await?;

    let runs = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::OperationStarted),
            operation_kind: Some(OperationKind::Run),
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        runs.iter().map(|record| record.id().to_owned()).collect::<Vec<_>>(),
        vec!["run-old", "run-new"]
    );
    let compactions = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::OperationStarted),
            operation_kind: Some(OperationKind::Compaction),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        compactions
            .iter()
            .map(|record| record.id().to_owned())
            .collect::<Vec<_>>(),
        vec!["compaction"]
    );
    let navigations = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::OperationStarted),
            operation_kind: Some(OperationKind::Navigation),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        navigations
            .iter()
            .map(|record| record.id().to_owned())
            .collect::<Vec<_>>(),
        vec!["navigation"]
    );
    let latest_run = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::OperationStarted),
            operation_kind: Some(OperationKind::Run),
            limit: Some(1),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        latest_run
            .iter()
            .map(|record| record.id().to_owned())
            .collect::<Vec<_>>(),
        vec!["run-new"]
    );
    Ok(())
}

pub(super) async fn tracks_and_enforces_one_open_operation_per_lane<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    assert!(session.find_open_operations("main", Some(2)).await?.is_empty());
    let first = session
        .append_record(operation_started("first", "main", OperationKind::Run))
        .await?;
    assert_eq!(
        session.find_open_operations("main", Some(2)).await?,
        vec![first.clone()]
    );
    assert_code(
        session
            .append_record(operation_started("second", "main", OperationKind::Run))
            .await,
        SessionErrorCode::Storage,
    );
    assert_eq!(
        session.find_open_operations("main", Some(2)).await?,
        vec![first.clone()]
    );
    session
        .append_record(operation_finished("finish-first", "main", first.id()))
        .await?;
    assert!(session.find_open_operations("main", Some(2)).await?.is_empty());
    Ok(())
}

pub(super) async fn does_not_let_an_earlier_finish_close_a_later_start<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_record(operation_finished("finish-before-start", "main", "run"))
        .await?;
    let started = session
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await?;
    assert_eq!(session.find_open_operations("main", Some(2)).await?, vec![started]);
    Ok(())
}

pub(super) async fn scopes_open_operations_by_lane_and_limit<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session.create_lane("thread", None).await?;
    let main_run = session
        .append_record(operation_started("main-run", "main", OperationKind::Run))
        .await?;
    let thread_navigation = session
        .append_record(operation_started(
            "thread-navigation",
            "thread",
            OperationKind::Navigation,
        ))
        .await?;
    assert_eq!(
        session.find_open_operations("main", None).await?,
        vec![main_run.clone()]
    );
    assert_eq!(session.find_open_operations("main", Some(1)).await?, vec![main_run]);
    assert_eq!(
        session.find_open_operations("thread", Some(2)).await?,
        vec![thread_navigation]
    );
    Ok(())
}

pub(super) async fn returns_immutable_open_operation_records<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let committed = session
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await?;
    let mut read = session.find_open_operations("main", None).await?;
    let RecordBody::OperationStarted {
        intent: OperationIntent::Run { original_prompt, .. },
        ..
    } = &mut read[0].body
    else {
        panic!("Expected an open run operation");
    };
    original_prompt.push(user_message("mutated"));
    assert_eq!(session.find_open_operations("main", None).await?, vec![committed]);
    Ok(())
}

fn usage(input: f64, output: f64, cache_read: f64, cache_write: f64, total: f64) -> Usage {
    Usage {
        input,
        output,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: total,
        cost: UsageCost {
            input: 1.0,
            output: 2.0,
            cache_read: 3.0,
            cache_write: 4.0,
            total: 10.0,
        },
    }
}

pub(super) async fn keeps_latest_value_facts_and_computes_ledger_statistics_across_lanes<
    R: SessionBackendRepository,
>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_entry(message_entry("user", user_message("question")), "main")
        .await?;
    let mut assistant = assistant_message("answer");
    if let AgentMessage::Assistant(message) = &mut assistant {
        message.usage = usage(10.0, 5.0, 3.0, 2.0, 20.0);
    }
    session
        .append_entry(message_entry("assistant", assistant), "main")
        .await?;
    let assistant_usage = usage(10.0, 5.0, 3.0, 2.0, 20.0);
    session
        .append_record(RecordBody::Usage {
            id: "assistant-usage".to_owned(),
            lane: "main".to_owned(),
            cause: UsageCause::Assistant,
            run_id: Some("run".to_owned()),
            entry_id: Some("assistant".to_owned()),
            attempt: Some(1.0),
            stop_reason: Some(SessionStopReason::Stop),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: assistant_usage,
        })
        .await?;
    session
        .append_record(RecordBody::Usage {
            id: "deferred-usage".to_owned(),
            lane: "main".to_owned(),
            cause: UsageCause::DeferredFetch,
            run_id: Some("run".to_owned()),
            entry_id: Some("deferred-result".to_owned()),
            attempt: Some(1.0),
            stop_reason: Some(SessionStopReason::Deferred),
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: Usage::default(),
        })
        .await?;
    session.create_lane("thread", Some("assistant")).await?;
    session
        .append_record(RecordBody::Usage {
            id: "correction".to_owned(),
            lane: "thread".to_owned(),
            cause: UsageCause::Adjustment,
            run_id: None,
            entry_id: None,
            attempt: None,
            stop_reason: None,
            tool_call_id: None,
            details: OptionalValue::Present(json!({"reason": "provider correction"}).into()),
            usage: Usage {
                input: -2.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                cache_write_1h: None,
                reasoning: None,
                total_tokens: -2.0,
                cost: UsageCost {
                    input: -0.5,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                    total: -0.5,
                },
            },
        })
        .await?;
    session.set_name(Some("First")).await?;
    session.set_name(Some("Second")).await?;
    session.set_label("user", Some("keep")).await?;
    session.set_label("user", None).await?;
    assert_code(
        session.set_label("missing", Some("checkpoint")).await,
        SessionErrorCode::NotFound,
    );

    assert_eq!(session.get_name().await?.as_deref(), Some("Second"));
    assert_eq!(session.get_label("user").await?, None);
    let usage_records = session
        .find_records(RecordQuery {
            record_type: Some(RecordType::Usage),
            order: Some(EntryOrder::OldestFirst),
            ..Default::default()
        })
        .await?;
    let causes: Vec<UsageCause> = usage_records
        .iter()
        .map(|record| match &record.body {
            RecordBody::Usage { cause, .. } => *cause,
            other => panic!("unexpected record {other:?}"),
        })
        .collect();
    assert_eq!(
        causes,
        vec![UsageCause::Assistant, UsageCause::DeferredFetch, UsageCause::Adjustment]
    );
    let deferred = usage_records
        .iter()
        .find(|record| {
            matches!(
                &record.body,
                RecordBody::Usage {
                    cause: UsageCause::DeferredFetch,
                    ..
                }
            )
        })
        .expect("deferred usage");
    match &deferred.body {
        RecordBody::Usage { stop_reason, .. } => assert_eq!(*stop_reason, Some(SessionStopReason::Deferred)),
        other => panic!("unexpected record {other:?}"),
    }
    assert_eq!(
        session.get_stats().await?,
        SessionStats {
            message_count: 2,
            cached_tokens: 3.0,
            uncached_tokens: 10.0,
            total_tokens: 18.0,
            cost_total: 9.5,
        }
    );
    Ok(())
}

pub(super) async fn clears_session_names_durably<R: SessionBackendRepository>(fixture: &R) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session.set_name(Some("Temporary")).await?;
    session.set_name(None).await?;
    assert_eq!(session.get_name().await?, None);
    assert_eq!(
        session.get_log(LogOptions::default()).await?,
        vec![
            LogItem::Fact {
                seq: 1,
                fact: crate::harness::session::FactItem::Name {
                    name: Some("Temporary".to_owned())
                }
            },
            LogItem::Fact {
                seq: 2,
                fact: crate::harness::session::FactItem::Name { name: None }
            },
        ]
    );
    let metadata = session.get_metadata().await?;
    let reopened = fixture.open(&metadata).await?;
    assert_eq!(reopened.get_name().await?, None);
    assert_eq!(
        reopened.get_log(LogOptions::default()).await?,
        session.get_log(LogOptions::default()).await?
    );
    let fork = fixture
        .fork(
            &metadata,
            ForkOptions::Branch,
            SessionCreateOptions {
                id: Some("fork".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(fork.get_name().await?, None);
    Ok(())
}

pub(super) async fn returns_immutable_copies_from_reads<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("immutable".into()),
            ..Default::default()
        })
        .await?;
    let metadata = session.get_metadata().await?;
    let mut data = json!({"nested": {"value": 1}});
    session
        .append_entry(
            EntryBody::Custom {
                id: "custom".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Present(data.clone().into()),
            },
            "main",
        )
        .await?;
    data["nested"]["value"] = json!(50);
    let mut read = session.get_entry("custom").await?.expect("entry");
    let EntryBody::Custom {
        data: OptionalValue::Present(value),
        ..
    } = &mut read.body
    else {
        panic!("Expected custom entry");
    };
    value["nested"]["value"] = json!(99).into();
    let mut read_metadata = session.get_metadata().await?;
    let mut changed = serde_json::to_value(&read_metadata).expect("metadata JSON");
    changed["id"] = json!("changed");
    read_metadata = serde_json::from_value(changed).expect("changed metadata");
    assert_eq!(read_metadata.id(), "changed");
    let expected_metadata = metadata.clone();
    let mut log = session.get_log(LogOptions::default()).await?;
    let LogItem::Entry {
        entry:
            Entry {
                body:
                    EntryBody::Custom {
                        data: OptionalValue::Present(value),
                        ..
                    },
                ..
            },
        ..
    } = &mut log[0]
    else {
        panic!("Expected entry log");
    };
    value["nested"]["value"] = json!(100).into();
    assert_eq!(session.get_metadata().await?, expected_metadata);
    let stored = session.get_entry("custom").await?.expect("entry");
    assert_eq!(
        stored,
        Entry {
            body: EntryBody::Custom {
                id: "custom".to_owned(),
                custom_type: "note".to_owned(),
                data: OptionalValue::Present(json!({"nested": {"value": 1}}).into()),
            },
            parent_id: None,
            seq: 1,
            timestamp: read.timestamp,
        }
    );
    Ok(())
}

pub(super) async fn validates_lane_lifecycle_and_targets<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    assert_code(session.create_lane("main", None).await, SessionErrorCode::AlreadyExists);
    assert_code(
        session.create_lane("thread", Some("missing")).await,
        SessionErrorCode::NotFound,
    );
    assert_code(session.move_lane("missing", None).await, SessionErrorCode::InvalidLane);
    Ok(())
}

pub(super) async fn binds_lane_views_without_caching_leaves<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let root = session.append_message(user_message("root")).await?;
    session.create_lane("thread", Some(&root)).await?;
    let thread = session.view("thread");
    let (main_child, thread_child) = tokio::join!(
        session.append_message(user_message("main")),
        thread.append_message(user_message("thread"))
    );
    let main_child = main_child?;
    let thread_child = thread_child?;
    assert_eq!(session.get_leaf_id().await?.as_deref(), Some(main_child.as_str()));
    assert_eq!(thread.get_leaf_id().await?.as_deref(), Some(thread_child.as_str()));
    let main_branch = session
        .find_entries_on_branch(BranchQuery {
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            },
            ..Default::default()
        })
        .await?;
    assert_eq!(ids(&main_branch), vec![root.clone(), main_child]);
    let thread_branch = thread
        .find_entries_on_branch(BranchQuery {
            entry: EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            },
            ..Default::default()
        })
        .await?;
    assert_eq!(ids(&thread_branch), vec![root, thread_child]);
    let empty = fixture
        .create(SessionCreateOptions {
            id: Some("empty".into()),
            ..Default::default()
        })
        .await?;
    assert!(empty.find_entries_on_branch(BranchQuery::default()).await?.is_empty());
    Ok(())
}

pub(super) async fn appends_provisioned_entries_with_their_existing_ids<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let entry = session
        .append_entry(custom_entry("provisioned", "note", Some(json!({"value": 1}))), "main")
        .await?;
    assert_eq!(entry.body.custom_type(), Some("note"));
    assert_eq!(
        (entry.id().to_owned(), entry.parent_id.clone(), entry.seq),
        ("provisioned".to_owned(), None, 1)
    );
    assert_eq!(session.get_leaf_id().await?.as_deref(), Some("provisioned"));
    Ok(())
}

pub(super) async fn persists_tool_result_termination_decisions<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    let tool_result = AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "example".to_owned(),
        content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
            "done",
        ))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 1,
    });
    let entry = session
        .append_entry(
            EntryBody::Message {
                id: "tool-result".to_owned(),
                message: tool_result,
                terminate: Some(true),
            },
            "main",
        )
        .await?;
    match &entry.body {
        EntryBody::Message { terminate, .. } => assert_eq!(*terminate, Some(true)),
        other => panic!("unexpected entry {other:?}"),
    }
    let stored = session.get_entry(entry.id()).await?.expect("entry");
    match &stored.body {
        EntryBody::Message { terminate, .. } => assert_eq!(*terminate, Some(true)),
        other => panic!("unexpected entry {other:?}"),
    }
    assert_eq!(session.find_entries(EntryQuery::default()).await?, vec![entry.clone()]);
    assert_eq!(
        session.get_log(LogOptions::default()).await?,
        vec![LogItem::Entry { seq: entry.seq, entry }]
    );
    Ok(())
}

async fn track_completion(
    write: impl std::future::Future<Output = Result<Entry, SessionError>>,
    completion_order: &std::sync::Mutex<Vec<String>>,
) -> Result<Entry, SessionError> {
    let entry = write.await?;
    completion_order
        .lock()
        .expect("completion lock")
        .push(entry.id().to_owned());
    Ok(entry)
}

pub(super) async fn linearizes_concurrent_writes_across_two_lanes<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("session".into()),
            ..Default::default()
        })
        .await?;
    session
        .append_entry(message_entry("root", user_message("root")), "main")
        .await?;
    session.create_lane("thread", Some("root")).await?;
    let completion_order = std::sync::Mutex::new(Vec::new());
    let (main_1, thread_1, main_2, thread_2) = tokio::join!(
        track_completion(
            session.append_entry(custom_entry("main-1", "note", None), "main"),
            &completion_order
        ),
        track_completion(
            session.append_entry(custom_entry("thread-1", "note", None), "thread"),
            &completion_order
        ),
        track_completion(
            session.append_entry(custom_entry("main-2", "note", None), "main"),
            &completion_order
        ),
        track_completion(
            session.append_entry(custom_entry("thread-2", "note", None), "thread"),
            &completion_order
        ),
    );
    let entries = [main_1?, thread_1?, main_2?, thread_2?];
    let mut sequences: Vec<i64> = entries.iter().map(|entry| entry.seq).collect();
    sequences.sort_unstable();
    sequences.dedup();
    assert_eq!(sequences.len(), entries.len());
    let mut commit_order: Vec<(i64, String)> = entries.iter().map(|entry| (entry.seq, entry.id().to_owned())).collect();
    commit_order.sort_by_key(|(seq, _)| *seq);
    assert_eq!(
        *completion_order.lock().expect("completion lock"),
        commit_order.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>()
    );
    let concurrent: Vec<String> = entries.iter().map(|entry| entry.id().to_owned()).collect();
    let logged: Vec<String> = session
        .get_log(LogOptions::default())
        .await?
        .into_iter()
        .filter_map(|item| match item {
            LogItem::Entry { entry, .. } if concurrent.contains(&entry.id().to_owned()) => Some(entry.id().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(
        logged,
        commit_order.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>()
    );
    let log_sequences: Vec<i64> = session
        .get_log(LogOptions::default())
        .await?
        .iter()
        .map(LogItem::seq)
        .collect();
    let mut sorted = log_sequences.clone();
    sorted.sort_unstable();
    assert_eq!(log_sequences, sorted);
    Ok(())
}

pub(super) async fn creates_lists_and_opens_sessions<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("one".into()),
            ..Default::default()
        })
        .await?;
    let entry_id = session.append_message(user_message("persisted")).await?;
    let metadata = session.get_metadata().await?;
    let listed = fixture.list().await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), metadata.id());
    assert_eq!(
        serde_json::to_value(&listed[0]).unwrap()["createdAt"],
        serde_json::to_value(&metadata).unwrap()["createdAt"]
    );
    assert_eq!(
        serde_json::to_value(&listed[0]).unwrap()["parentSessionId"],
        serde_json::to_value(&metadata).unwrap()["parentSessionId"]
    );
    let reopened = fixture.open(&metadata).await?;
    assert_eq!(
        ids(&reopened.find_entries(EntryQuery::default()).await?),
        vec![entry_id]
    );
    assert_code(
        fixture
            .create(SessionCreateOptions {
                id: Some("one".to_owned()),
                ..Default::default()
            })
            .await,
        SessionErrorCode::AlreadyExists,
    );
    Ok(())
}

pub(super) async fn deletes_sessions_idempotently<R: SessionBackendRepository>(fixture: &R) -> ConformanceResult<()> {
    let session = fixture
        .create(SessionCreateOptions {
            id: Some("one".into()),
            ..Default::default()
        })
        .await?;
    let metadata = session.get_metadata().await?;
    fixture.delete(&metadata).await?;
    assert_code(fixture.open(&metadata).await, SessionErrorCode::NotFound);
    fixture.delete(&metadata).await?;
    Ok(())
}

pub(super) async fn forks_one_branch_with_selected_facts_and_no_records<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let source = fixture
        .create(SessionCreateOptions {
            id: Some("source".into()),
            ..Default::default()
        })
        .await?;
    let root = source.append_message(user_message("root")).await?;
    let shared = source.append_message(assistant_message("shared")).await?;
    source.create_lane("thread", Some(&shared)).await?;
    let thread_child = source.view("thread").append_message(user_message("thread")).await?;
    let main_child = source.append_message(user_message("main")).await?;
    source.set_name(Some("Source")).await?;
    source.set_label(&shared, Some("copied")).await?;
    source.set_label(&thread_child, Some("excluded")).await?;
    source
        .append_record(operation_started("run", "main", OperationKind::Run))
        .await?;
    source
        .append_record(RecordBody::Usage {
            id: "source-usage".to_owned(),
            lane: "main".to_owned(),
            cause: UsageCause::Adjustment,
            run_id: None,
            entry_id: None,
            attempt: None,
            stop_reason: None,
            tool_call_id: None,
            details: OptionalValue::Missing,
            usage: usage(10.0, 5.0, 3.0, 2.0, 20.0),
        })
        .await?;

    let fork = fixture
        .fork(
            &source.get_metadata().await?,
            ForkOptions::BranchAt {
                entry_id: main_child.clone(),
            },
            SessionCreateOptions {
                id: Some("branch-fork".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        ids(&fork
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await?),
        vec![root, shared.clone(), main_child.clone()]
    );
    assert_eq!(
        lanes(&fork.get_lanes().await?),
        vec![("main".to_owned(), Some(main_child.clone()))]
    );
    assert_eq!(fork.get_name().await?.as_deref(), Some("Source"));
    assert_eq!(fork.get_label(&shared).await?.as_deref(), Some("copied"));
    assert_eq!(fork.get_label(&thread_child).await?, None);
    assert!(fork.find_records(RecordQuery::default()).await?.is_empty());
    assert_eq!(
        fork.get_stats().await?,
        SessionStats {
            message_count: 3,
            cached_tokens: 0.0,
            uncached_tokens: 0.0,
            total_tokens: 0.0,
            cost_total: 0.0,
        }
    );
    fork.append_message(user_message("after fork")).await?;
    assert_eq!(fork.get_stats().await?.message_count, 4);
    let metadata = fork.get_metadata().await?;
    assert_eq!(metadata.id(), "branch-fork");
    assert_eq!(
        serde_json::to_value(&metadata).unwrap()["parentSessionId"],
        json!("source")
    );
    Ok(())
}

pub(super) async fn forks_a_complete_tree_with_lanes_and_facts<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let source = fixture
        .create(SessionCreateOptions {
            id: Some("source".into()),
            ..Default::default()
        })
        .await?;
    let root = source.append_message(user_message("root")).await?;
    source.create_lane("thread", Some(&root)).await?;
    let main_child = source.append_message(user_message("main")).await?;
    let thread_child = source.view("thread").append_message(user_message("thread")).await?;
    source.set_label(&thread_child, Some("thread-tip")).await?;

    let fork = fixture
        .fork(
            &source.get_metadata().await?,
            ForkOptions::Tree,
            SessionCreateOptions {
                id: Some("tree-fork".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        ids(&fork
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await?),
        vec![root, main_child.clone(), thread_child.clone()]
    );
    assert_eq!(
        lanes(&fork.get_lanes().await?),
        vec![
            ("main".to_owned(), Some(main_child.clone())),
            ("thread".to_owned(), Some(thread_child.clone())),
        ]
    );
    assert_eq!(fork.get_label(&thread_child).await?.as_deref(), Some("thread-tip"));
    assert_eq!(fork.get_stats().await?.message_count, 3);
    let lane_items: Vec<LogItem> = fork
        .get_log(LogOptions::default())
        .await?
        .into_iter()
        .filter(|item| matches!(item, LogItem::Lane { .. }))
        .collect();
    assert_eq!(
        lane_items,
        vec![
            LogItem::Lane {
                seq: 4,
                lane: "main".to_owned(),
                leaf_id: Some(main_child)
            },
            LogItem::Lane {
                seq: 5,
                lane: "thread".to_owned(),
                leaf_id: Some(thread_child)
            },
        ]
    );
    Ok(())
}

pub(super) async fn forks_before_an_entry_without_modifying_the_source<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let source = fixture
        .create(SessionCreateOptions {
            id: Some("source".into()),
            ..Default::default()
        })
        .await?;
    let root = source.append_message(user_message("root")).await?;
    let tail = source.append_message(user_message("tail")).await?;
    let fork = fixture
        .fork(
            &source.get_metadata().await?,
            ForkOptions::BranchBefore { entry_id: tail.clone() },
            SessionCreateOptions {
                id: Some("fork".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        ids(&fork
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await?),
        vec![root.clone()]
    );
    assert_eq!(fork.get_leaf_id().await?.as_deref(), Some(root.as_str()));
    assert_eq!(source.get_leaf_id().await?.as_deref(), Some(tail.as_str()));
    let before_default = fixture
        .fork(
            &source.get_metadata().await?,
            ForkOptions::BranchDefaultBefore,
            SessionCreateOptions {
                id: Some("before-default-target".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        ids(&before_default
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await?),
        vec![root.clone()]
    );
    assert_eq!(before_default.get_leaf_id().await?.as_deref(), Some(root.as_str()));
    let at_default = fixture
        .fork(
            &source.get_metadata().await?,
            ForkOptions::Branch,
            SessionCreateOptions {
                id: Some("at-default-target".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(
        ids(&at_default
            .find_entries(EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..Default::default()
            })
            .await?),
        vec![root, tail.clone()]
    );
    assert_eq!(at_default.get_leaf_id().await?.as_deref(), Some(tail.as_str()));
    assert_code(
        fixture
            .fork(
                &source.get_metadata().await?,
                ForkOptions::BranchAt {
                    entry_id: "missing".to_owned(),
                },
                SessionCreateOptions { ..Default::default() },
            )
            .await,
        SessionErrorCode::InvalidForkTarget,
    );
    Ok(())
}

pub(super) async fn validates_the_default_fork_target<R: SessionBackendRepository>(
    fixture: &R,
) -> ConformanceResult<()> {
    let source = fixture
        .create(SessionCreateOptions {
            id: Some("source-with-custom-leaf".into()),
            ..Default::default()
        })
        .await?;
    source.append_custom_entry("not-a-message", None).await?;
    assert_code(
        fixture
            .fork(
                &source.get_metadata().await?,
                ForkOptions::Branch,
                SessionCreateOptions {
                    id: Some("fork".to_owned()),
                    ..Default::default()
                },
            )
            .await,
        SessionErrorCode::InvalidForkTarget,
    );
    Ok(())
}
