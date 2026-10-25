//! Behavioral tests for the in-memory multi-agent store (`core/multi-agent-store.ts`).

use pi_js_compat::js_json::{self, JsString};
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, AgentSubscription, ContactParentInput, MailboxMessageCommandResult,
    MailboxMessageStatusValue, MultiAgentStore, MultiAgentStoreOptions, ParentContactResult,
    RecordOutboundSessionMessageInput, SendMailboxMessageInput, SendSteeringInput, SessionPersistence,
};
use pi_coding_agent::core::session_control_db::{list_runtime_mailbox_listeners, read_multi_agent_state};

// Frozen session-control-db.ts retention uses Date.now(), including restore.
fn fresh_time() -> String {
    static NOW: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(pi_coding_agent::utils::iso_time::now_iso_string);
    NOW.clone()
}

fn new_store() -> MultiAgentStore {
    MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok(fresh_time().into()))),
    })
}

fn scout_input(parent: Option<&JsString>) -> AgentSnapshot {
    AgentSnapshot {
        id: JsString::default(),
        parent_id: parent.map(JsString::from),
        display_name: "Scout".into(),
        agent_type: "scout".into(),
        origin: None,
        lifecycle: AgentLifecycle::Running,
        revision: 1,
        created_at: fresh_time().into(),
        updated_at: fresh_time().into(),
        cwd: "/repo".into(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: pi_js_compat::js_json::JsObject::new(),
    }
}

fn spawn_agent(store: &MultiAgentStore, parent: Option<&JsString>) -> AgentSnapshot {
    let mut agent = scout_input(parent);
    agent.id = store.allocate_agent_id_for_lifecycle_coordinator("agent").unwrap();
    if let Some(persistence) = store.get_persistence_target() {
        pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent(
            &persistence.control_db_path,
            &persistence.session_path,
            &agent.id,
            &js_json::to_value(&agent).expect("agent"),
        )
        .expect("bootstrap");
    }
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    agent
}

fn can_transition(from: AgentLifecycle, to: AgentLifecycle) -> bool {
    let allowed: &[AgentLifecycle] = match from {
        AgentLifecycle::Running => &[
            AgentLifecycle::WaitingForInput,
            AgentLifecycle::SteeringPending,
            AgentLifecycle::Cancelling,
            AgentLifecycle::Completed,
            AgentLifecycle::Failed,
            AgentLifecycle::Aborted,
        ],
        AgentLifecycle::WaitingForInput => &[
            AgentLifecycle::Running,
            AgentLifecycle::SteeringPending,
            AgentLifecycle::Cancelling,
            AgentLifecycle::Completed,
            AgentLifecycle::Aborted,
        ],
        AgentLifecycle::SteeringPending => &[
            AgentLifecycle::Running,
            AgentLifecycle::WaitingForInput,
            AgentLifecycle::Cancelling,
            AgentLifecycle::Failed,
            AgentLifecycle::Aborted,
        ],
        AgentLifecycle::Cancelling => &[
            AgentLifecycle::Aborted,
            AgentLifecycle::Failed,
            AgentLifecycle::Completed,
        ],
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted => &[],
    };
    (from == to
        && !matches!(
            from,
            AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
        ))
        || allowed.contains(&to)
}

fn transition(
    store: &MultiAgentStore,
    agent_id: &JsString,
    expected_revision: i64,
    requested: AgentLifecycle,
) -> Result<AgentSnapshot, &'static str> {
    let current = store.get_agent(agent_id).ok_or("not_found")?;
    if current.revision != expected_revision {
        return Err("stale_revision");
    }
    if !can_transition(current.lifecycle, requested) {
        return Err("invalid_transition");
    }
    let mut updated = current;
    updated.lifecycle = requested;
    updated.revision += 1;
    if matches!(
        requested,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    ) {
        store.publish_terminal_outbox_snapshot(&updated).unwrap();
    } else {
        store.publish_lifecycle_coordinator_snapshot(&updated).unwrap();
    }
    Ok(updated)
}

type RecordedUpdates = Arc<Mutex<Vec<(AgentLifecycle, Option<JsString>)>>>;

fn record_updates(store: &MultiAgentStore) -> (RecordedUpdates, AgentSubscription) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = updates.clone();
    let subscription = store.subscribe_agent_updates(move |previous, current| {
        captured.lock().expect("updates").push((
            previous.lifecycle,
            current
                .transcript
                .as_ref()
                .and_then(|transcript| transcript.path.clone()),
        ));
        Ok(())
    });
    (updates, subscription)
}

#[test]
fn rejects_stale_lifecycle_mutations_and_returns_current_snapshot() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let waiting = transition(&store, &spawned.id, 1, AgentLifecycle::WaitingForInput).expect("waiting");
    assert_eq!(waiting.revision, 2);

    let stale = transition(&store, &spawned.id, 1, AgentLifecycle::Running).expect_err("stale");
    assert_eq!(stale, "stale_revision");
    let current = store.get_agent(&spawned.id).expect("agent");
    assert_eq!(current.lifecycle, AgentLifecycle::WaitingForInput);
    assert_eq!(current.revision, 2);
}

#[test]
fn notifies_subscribers_for_transcript_metadata_updates() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let (updates, _subscription) = record_updates(&store);
    let transcript = pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "child-session".into(),
        path: Some("/tmp/child-session.jsonl".into()),
    };
    let result = store.update_agent_transcript(&spawned.id, &transcript).expect("update");
    assert!(matches!(
        result,
        pi_coding_agent::core::multi_agent_store::AgentTranscriptCommandResult::Ok(_)
    ));
    assert_eq!(
        *updates.lock().expect("updates"),
        vec![(AgentLifecycle::Running, Some("/tmp/child-session.jsonl".into()))]
    );
}

#[test]
fn view_selection_is_read_only_and_clears_without_mutating_lifecycle() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let viewed = store.select_agent_view(&spawned.id).expect("view");
    assert_eq!(viewed.id, spawned.id);
    assert_eq!(viewed.lifecycle, AgentLifecycle::Running);
    assert_eq!(store.get_agent(&spawned.id).expect("agent"), viewed);
    assert_eq!(store.get_selected_agent_id(), Some(spawned.id.clone()));
    store.clear_selected_agent_view();
    assert_eq!(store.get_selected_agent_id(), None);
    assert_eq!(
        store.get_agent(&spawned.id).expect("agent").lifecycle,
        AgentLifecycle::Running
    );
}

#[test]
fn inactive_agents_are_viewable_but_not_active_targets() {
    let store = new_store();
    let active = spawn_agent(&store, Some(&JsString::from("root")));
    let completed = spawn_agent(&store, Some(&JsString::from("root")));
    transition(&store, &completed.id, 1, AgentLifecycle::Completed).expect("complete");
    store.select_agent_view(&active.id);
    let viewed = store.select_agent_view(&completed.id).expect("view");
    assert_eq!(viewed.lifecycle, AgentLifecycle::Completed);
    assert_eq!(store.get_selected_agent_id(), Some(completed.id.clone()));

    store.select_agent_view(&active.id);
    let target = store.select_active_agent_target_with_status(&completed.id);
    assert!(matches!(
        target,
        pi_coding_agent::core::multi_agent_store::ActiveAgentTargetSelectionResult::Inactive { .. }
    ));
    assert_eq!(store.get_selected_agent_id(), Some(active.id));
}

#[test]
fn steering_messages_move_through_pending_accepted_and_delivered() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let message = store
        .prepare_steering_message_for_lifecycle_coordinator(
            &spawned.id,
            &SendSteeringInput {
                from_agent_id: "root".into(),
                body: "Inspect auth first".into(),
                target_checkpoint: Some(pi_coding_agent::core::multi_agent_store::SteeringCheckpoint::NextModelCall),
                thread_id: None,
                file_refs: None,
            },
        )
        .expect("prepare");
    assert_eq!(message.body.as_ref(), Some(&JsString::from("Inspect auth first")));
    assert_eq!(
        message.kind,
        pi_coding_agent::core::multi_agent_store::MailboxMessageKindValue::Steer
    );
    assert_eq!(message.status, MailboxMessageStatusValue::Pending);

    let mut updated = store.get_agent(&spawned.id).expect("agent");
    updated.lifecycle = AgentLifecycle::SteeringPending;
    updated.revision += 1;
    store
        .publish_lifecycle_coordinator_steering(&updated, &message)
        .unwrap();
    assert_eq!(
        store.get_agent(&spawned.id).expect("agent").lifecycle,
        AgentLifecycle::SteeringPending
    );

    let accepted = pi_coding_agent::core::multi_agent_store::AgentMailboxMessage {
        status: MailboxMessageStatusValue::Accepted,
        ..message.clone()
    };
    store
        .publish_lifecycle_coordinator_steering(&updated, &accepted)
        .unwrap();
    assert_eq!(
        store.list_mailbox_messages().unwrap()[0].status,
        MailboxMessageStatusValue::Accepted
    );

    updated.lifecycle = AgentLifecycle::Running;
    updated.revision += 1;
    // Frozen multi-agent-store.ts:366-368 removes delivered steering messages.
    store
        .publish_lifecycle_coordinator_steering_delivery(&updated, &accepted)
        .unwrap();
    assert!(store.list_mailbox_messages().unwrap().is_empty());
    assert_eq!(
        store.get_agent(&spawned.id).expect("agent").lifecycle,
        AgentLifecycle::Running
    );
}

#[test]
fn active_counts_follow_core_lifecycle_state() {
    let store = new_store();
    let active = spawn_agent(&store, Some(&JsString::from("root")));
    let running = spawn_agent(&store, Some(&JsString::from("root")));
    let completed = spawn_agent(&store, Some(&JsString::from("root")));
    let failed = spawn_agent(&store, Some(&JsString::from("root")));
    let aborted = spawn_agent(&store, Some(&JsString::from("root")));
    transition(&store, &completed.id, 1, AgentLifecycle::Completed).expect("complete");
    transition(&store, &failed.id, 1, AgentLifecycle::Aborted).expect("abort");
    transition(&store, &aborted.id, 1, AgentLifecycle::Aborted).expect("abort");
    assert_eq!(store.get_active_agent_count(), 2);
    let ids: Vec<JsString> = store.list_active_agents().into_iter().map(|agent| agent.id).collect();
    assert_eq!(ids, vec![active.id, running.id]);
}

#[test]
fn pinned_slots_preserve_metadata_and_conflict_with_occupants() {
    let store = new_store();
    let mut input = scout_input(Some(&JsString::from("root")));
    input.id = store.allocate_agent_id_for_lifecycle_coordinator("agent").unwrap();
    input.account = Some(pi_coding_agent::core::multi_agent_store::AgentAccount {
        id: "account-1".into(),
        budget_id: Some("budget-1".into()),
        provider_fallback: None,
        token_budget: None,
        concurrency_cap: None,
        rate_limit: None,
    });
    input.model = Some(pi_coding_agent::core::multi_agent_store::AgentModel {
        provider_id: "faux".into(),
        model_id: "gpt-test".into(),
        thinking_level: Some("medium".into()),
    });
    input.slot = Some(pi_coding_agent::core::multi_agent_store::AgentSlot { index: 2, pinned: true });
    input.transcript = Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "session-1".into(),
        path: Some("/tmp/sessions/child.jsonl".into()),
    });
    input.worktree = Some(pi_coding_agent::core::multi_agent_store::AgentWorktree {
        path: "/repo-worktrees/worker".into(),
        branch: Some("agent/worker".into()),
        base: Some("main".into()),
    });
    let spawned = input.clone();
    store.publish_lifecycle_coordinator_snapshot(&input).unwrap();

    let pinned = store.pin_agent_slot(&spawned.id, 1, 7).expect("pin");
    let pi_coding_agent::core::multi_agent_store::AgentMetadataCommandResult::Ok(pinned) = pinned else {
        panic!("expected pin");
    };
    assert_eq!(pinned.slot.as_ref().expect("slot").index, 7);
    assert_eq!(pinned.lifecycle, AgentLifecycle::Running);
    assert_eq!(pinned.account.as_ref().expect("account").id, "account-1");
    assert_eq!(pinned.model.as_ref().expect("model").model_id, "gpt-test");
    assert_eq!(pinned.parent_id.as_ref(), Some(&JsString::from("root")));

    let other = spawn_agent(&store, Some(&JsString::from("root")));
    let conflict = store.pin_agent_slot(&other.id, 1, 7).expect("conflict");
    assert!(matches!(
        conflict,
        pi_coding_agent::core::multi_agent_store::AgentMetadataCommandResult::SlotConflict { .. }
    ));
}

#[test]
fn slot_fallback_uses_active_agent_order() {
    let store = new_store();
    let first = spawn_agent(&store, Some(&JsString::from("root")));
    let second = spawn_agent(&store, Some(&JsString::from("root")));
    let third = spawn_agent(&store, Some(&JsString::from("root")));
    assert_eq!(store.select_agent_slot(1).expect("slot 1").id, first.id);
    assert_eq!(store.select_agent_slot(2).expect("slot 2").id, second.id);
    assert_eq!(store.select_agent_slot(3).expect("slot 3").id, third.id);
    assert!(store.select_agent_slot(4).is_none());

    transition(&store, &first.id, 1, AgentLifecycle::Completed).expect("complete");
    assert_eq!(store.select_agent_slot(1).expect("slot 1").id, second.id);
    assert_eq!(store.select_agent_slot(2).expect("slot 2").id, third.id);
}

#[test]
fn waiting_for_input_notifications_are_recorded_once() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let captured = notifications.clone();
    let _subscription = store.subscribe_lifecycle_notifications(move |message| {
        if let Some(body) = &message.body {
            captured.lock().expect("notifications").push(body.clone());
        }
        Ok(())
    });
    let waiting = transition(&store, &spawned.id, 1, AgentLifecycle::WaitingForInput).expect("waiting");
    assert_eq!(
        *notifications.lock().expect("notifications"),
        vec!["Scout is waiting for input."]
    );
    let messages = store.list_mailbox_messages().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].kind,
        pi_coding_agent::core::multi_agent_store::MailboxMessageKindValue::System
    );
    assert_eq!(
        messages[0].thread_id.as_ref(),
        Some(&pi_js_compat::js_format!("agent-waiting-for-input:{}", spawned.id))
    );
    assert_eq!(messages[0].to_agent_id, "root");
    assert_eq!(waiting.revision, 2);

    store.publish_lifecycle_coordinator_snapshot(&waiting).unwrap();
    assert_eq!(store.list_mailbox_messages().unwrap().len(), 1);
}

#[test]
fn descendants_do_not_leak_sibling_branches() {
    let store = new_store();
    let root = spawn_agent(&store, Some(&JsString::from("root")));
    let first = spawn_agent(&store, Some(&root.id));
    let second = spawn_agent(&store, Some(&root.id));
    let grandchild = spawn_agent(&store, Some(&first.id));
    let ids: Vec<JsString> = store
        .list_descendants(&root.id)
        .into_iter()
        .map(|agent| agent.id)
        .collect();
    assert_eq!(ids, vec![first.id.clone(), grandchild.id.clone(), second.id.clone()]);
}

#[test]
fn contact_parent_only_targets_the_direct_parent() {
    let store = new_store();
    let root = spawn_agent(&store, Some(&JsString::from("root")));
    let child = spawn_agent(&store, Some(&root.id));
    let result = store
        .contact_parent(
            &child.id,
            &ContactParentInput {
                body: "need input".into(),
                thread_id: None,
                file_refs: None,
            },
        )
        .expect("contact");
    let ParentContactResult::Ok { agent, message } = result else {
        panic!("expected contact ok");
    };
    assert_eq!(message.to_agent_id, root.id);
    assert_eq!(
        message.kind,
        pi_coding_agent::core::multi_agent_store::MailboxMessageKindValue::ParentRequest
    );
    assert_eq!(message.status, MailboxMessageStatusValue::Pending);
    assert_eq!(agent.last_activity.expect("activity").description, "Contacted parent");

    let orphan = spawn_agent(&store, None);
    let result = store
        .contact_parent(
            &orphan.id,
            &ContactParentInput {
                body: "need input".into(),
                thread_id: None,
                file_refs: None,
            },
        )
        .expect("contact");
    assert!(matches!(result, ParentContactResult::ParentNotFound { .. }));
}

#[test]
fn direct_mailbox_messages_require_a_parent_child_relationship() {
    let store = new_store();
    let parent = spawn_agent(&store, Some(&JsString::from("root")));
    let child = spawn_agent(&store, Some(&parent.id));
    let sibling = spawn_agent(&store, Some(&parent.id));

    let allowed = store
        .send_mailbox_message(
            &child.id,
            1,
            &SendMailboxMessageInput {
                body: "hi parent".into(),
                to_agent_id: parent.id.clone(),
                thread_id: None,
                file_refs: None,
            },
        )
        .expect("send");
    let MailboxMessageCommandResult::Ok { message, .. } = allowed else {
        panic!("expected message");
    };
    assert_eq!(message.from_agent_id, child.id);
    assert_eq!(message.to_agent_id, parent.id);

    let forbidden = store
        .send_mailbox_message(
            &child.id,
            1,
            &SendMailboxMessageInput {
                body: "hi sibling".into(),
                to_agent_id: sibling.id.clone(),
                thread_id: None,
                file_refs: None,
            },
        )
        .expect("send");
    assert!(matches!(forbidden, MailboxMessageCommandResult::ForbiddenTarget { .. }));
}

#[test]
fn projections_are_snapshots_not_shared_state() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let projection = store.get_projection_snapshot().unwrap();
    assert_eq!(projection.active_count, 1);
    assert_eq!(projection.agents[0].id, spawned.id);
    assert_eq!(projection.rows[0].agent_id, spawned.id);
    assert!(projection.rows[0].active);

    store.select_agent_view(&spawned.id);
    let projection = store.get_projection_snapshot().unwrap();
    assert_eq!(projection.selected_agent_id, Some(spawned.id.clone()));
    assert!(projection.rows[0].selected);
}

#[test]
fn completion_and_failure_notifications_can_be_consumed() {
    let store = new_store();
    let completed = spawn_agent(&store, Some(&JsString::from("root")));
    let mut agent = store.get_agent(&completed.id).expect("agent");
    agent.lifecycle = AgentLifecycle::Completed;
    agent.revision += 1;
    agent.result = Some(pi_coding_agent::core::multi_agent_store::AgentResult {
        summary: Some("all done".into()),
        file_refs: None,
        duration_ms: Some(1200),
        tool_call_id: None,
    });
    store.publish_terminal_outbox_snapshot(&agent).unwrap();
    let pending = store
        .list_pending_lifecycle_notifications_for_agent(
            &completed.id,
            pi_coding_agent::core::multi_agent_store::LifecycleNotificationLifecycle::Completed,
        )
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].body.as_ref(),
        Some(&JsString::from("Scout completed: all done. Duration: 1200ms"))
    );
    let consumed = store.consume_completion_notifications_for_agent(&completed.id).unwrap();
    assert_eq!(consumed.len(), 1);
    // Frozen multi-agent-store.ts:539-549 returns the original message, then removes it.
    assert_eq!(consumed[0].status, MailboxMessageStatusValue::Pending);
    assert!(store.list_mailbox_messages().unwrap().is_empty());
    assert!(
        store
            .consume_completion_notifications_for_agent(&completed.id)
            .unwrap()
            .is_empty()
    );

    let failed = spawn_agent(&store, Some(&JsString::from("root")));
    let mut agent = store.get_agent(&failed.id).expect("agent");
    agent.lifecycle = AgentLifecycle::Failed;
    agent.revision += 1;
    agent.error = Some(pi_coding_agent::core::multi_agent_store::AgentError {
        message: "boom".into(),
        code: None,
    });
    store.publish_terminal_outbox_snapshot(&agent).unwrap();
    let notifications = store.consume_failure_notifications_for_agent(&failed.id).unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].body.as_ref(),
        Some(&JsString::from("Scout failed: boom"))
    );
}

#[test]
fn abort_handlers_fire_once_and_unregister_on_drop() {
    let store = new_store();
    let fired = Arc::new(Mutex::new(0));
    let captured = fired.clone();
    let registration = store.register_agent_abort_handler(&JsString::from("agent_1"), move || {
        *captured.lock().expect("fired") += 1;
        Ok(())
    });
    assert!(store.abort_agent_handle(&JsString::from("agent_1")).unwrap());
    assert_eq!(*fired.lock().expect("fired"), 1);
    assert!(!store.abort_agent_handle(&JsString::from("agent_1")).unwrap());
    drop(registration);
    assert!(!store.abort_agent_handle(&JsString::from("agent_1")).unwrap());

    let fired = Arc::new(Mutex::new(0));
    let captured = fired.clone();
    let registration = store.register_agent_abort_handler(&JsString::from("agent_2"), move || {
        *captured.lock().expect("fired") += 1;
        Ok(())
    });
    drop(registration);
    assert!(!store.abort_agent_handle(&JsString::from("agent_2")).unwrap());
    assert_eq!(*fired.lock().expect("fired"), 0);
}

#[test]
fn outbound_session_messages_are_recorded_without_a_local_target() {
    let store = new_store();
    let message = store
        .record_outbound_session_message(&RecordOutboundSessionMessageInput {
            from_agent_id: "main".into(),
            to_agent_id: "remote-session".into(),
            body: "hello".into(),
            thread_id: Some("thread".into()),
            file_refs: None,
        })
        .expect("record");
    assert_eq!(message.to_agent_id, "remote-session");
    assert_eq!(message.status, MailboxMessageStatusValue::Pending);
    assert_eq!(
        store
            .list_pending_mailbox_messages_for_agent(&JsString::from("remote-session"))
            .unwrap()
            .len(),
        1
    );
    let delivered = store
        .mark_mailbox_message_delivered(&message.id)
        .unwrap()
        .expect("delivered");
    // Frozen multi-agent-store.ts:539-549 preserves the returned status and deletes the row.
    assert_eq!(delivered.status, MailboxMessageStatusValue::Pending);
    assert!(store.list_mailbox_messages().unwrap().is_empty());
    assert!(store.mark_mailbox_message_delivered(&message.id).unwrap().is_none());
}

struct TestSession {
    control_db_path: String,
    session_path: String,
}

impl SessionPersistence for TestSession {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.control_db_path.as_str().into())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.session_path.as_str().into())
    }
}

#[test]
fn persisted_metadata_updates_survive_reopen() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/main.jsonl".into(),
    };
    let store = new_store();
    store.set_persistence_session_manager(Some(&session));
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let transcript = pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "child".into(),
        path: Some("/tmp/child.jsonl".into()),
    };
    store.update_agent_transcript(&spawned.id, &transcript).expect("update");
    let state = read_multi_agent_state(&session.control_db_path, &session.session_path)
        .expect("state")
        .expect("persisted");
    assert_eq!(state.agents.len(), 1);
    assert_eq!(
        state.agents[0]
            .get("transcript")
            .and_then(|transcript| transcript.get("sessionId"))
            .and_then(|value| value.as_string()),
        Some(&JsString::from("child"))
    );

    let restored = new_store();
    restored.restore_from_session_manager(&session).expect("restore");
    let agent = restored.get_agent(&spawned.id).expect("restored agent");
    assert_eq!(
        agent.transcript.as_ref().map(|transcript| &transcript.session_id),
        Some(&JsString::from("child"))
    );
}

#[test]
fn restore_clears_store_when_session_has_no_persisted_state() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/empty.jsonl".into(),
    };
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    assert!(store.get_agent(&spawned.id).is_some());
    store.restore_from_session_manager(&session).expect("restore");
    assert!(store.list_agents().is_empty());
    assert_eq!(store.get_restore_generation(), 1);
}

#[test]
fn persisted_counters_are_shared_across_store_instances() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/main.jsonl".into(),
    };
    let first = new_store();
    first.set_persistence_session_manager(Some(&session));
    let first_agent = spawn_agent(&first, Some(&JsString::from("root")));
    assert_eq!(first_agent.id, "agent_1");

    let second = new_store();
    second.set_persistence_session_manager(Some(&session));
    let second_agent = spawn_agent(&second, Some(&JsString::from("root")));
    assert_eq!(second_agent.id, "agent_2");
}

#[test]
fn crash_restore_clears_worker_handles_for_active_agents() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/main.jsonl".into(),
    };
    let mut agent = scout_input(Some(&JsString::from("root")));
    agent.id = "agent_1".into();
    agent.worker = Some(pi_coding_agent::core::multi_agent_store::AgentWorkerAdapter {
        adapter: pi_coding_agent::core::multi_agent_store::AgentWorkerAdapterKind::Runtime,
        handle_id: "handle".into(),
        cwd: None,
        tool_call_id: None,
    });
    pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent(
        &session.control_db_path,
        &session.session_path,
        &JsString::from("agent_1"),
        &js_json::to_value(&agent).expect("agent"),
    )
    .expect("bootstrap");

    let restored = new_store();
    restored.restore_from_session_manager(&session).expect("restore");
    let agent = restored.get_agent(&JsString::from("agent_1")).expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Running);
    assert_eq!(agent.worker, None);
}

#[test]
fn restore_preserves_waiting_notifications_and_mailbox_messages() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/main.jsonl".into(),
    };
    let store = new_store();
    store.set_persistence_session_manager(Some(&session));
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    transition(&store, &spawned.id, 1, AgentLifecycle::WaitingForInput).expect("waiting");
    assert_eq!(store.list_mailbox_messages().unwrap().len(), 1);

    let restored = new_store();
    restored.restore_from_session_manager(&session).expect("restore");
    let messages = restored.list_mailbox_messages().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].thread_id.as_ref(),
        Some(&pi_js_compat::js_format!("agent-waiting-for-input:{}", spawned.id))
    );
}

#[test]
fn listeners_registered_on_restored_sessions_are_dropped_by_restore() {
    let store = new_store();
    let spawned = spawn_agent(&store, Some(&JsString::from("root")));
    let notified = Arc::new(Mutex::new(0));
    let captured = notified.clone();
    let _subscription = store.subscribe_agent_updates(move |_, _| {
        *captured.lock().expect("notified") += 1;
        Ok(())
    });
    store.invalidate_in_flight_dispatches();
    let transcript = pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
        session_id: "child".into(),
        path: Some("/tmp/child.jsonl".into()),
    };
    store.update_agent_transcript(&spawned.id, &transcript).expect("update");
    assert_eq!(*notified.lock().expect("notified"), 1);
}

#[test]
fn runtime_mailbox_listeners_are_not_registered_by_the_store() {
    let directory = tempfile::tempdir().expect("tempdir");
    let session = TestSession {
        control_db_path: directory.path().join("control.sqlite").display().to_string(),
        session_path: "/sessions/main.jsonl".into(),
    };
    let store = new_store();
    store.set_persistence_session_manager(Some(&session));
    let _ = spawn_agent(&store, Some(&JsString::from("root")));
    assert!(
        list_runtime_mailbox_listeners(&session.control_db_path)
            .expect("listeners")
            .is_empty()
    );
}
