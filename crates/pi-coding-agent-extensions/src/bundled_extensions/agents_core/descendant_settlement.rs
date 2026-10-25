//! Persisted descendant reconciliation before child terminalization.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{self, JsString};

use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore, is_active_lifecycle};
use crate::core::session_control_db::read_multi_agent_state;

const DESCENDANT_RECONCILIATION_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub async fn wait_for_active_descendants(
    store: &MultiAgentStore,
    agent_id: &JsString,
    signal: Option<&AbortSignal>,
) -> Result<(), JsString> {
    if should_finish(store, agent_id, signal)? {
        return Ok(());
    }
    let notify = Arc::new(tokio::sync::Notify::new());
    let listener = Arc::clone(&notify);
    let _subscription = store.subscribe_agent_updates(move |_, _| {
        listener.notify_one();
        Ok(())
    });
    loop {
        if should_finish(store, agent_id, signal)? {
            return Ok(());
        }
        let cancelled = async {
            match signal {
                Some(signal) if !signal.aborted() => {
                    signal.wait().await;
                }
                _ => std::future::pending().await,
            }
        };
        tokio::select! {
            () = cancelled => {},
            () = notify.notified() => {},
            () = tokio::time::sleep(DESCENDANT_RECONCILIATION_POLL_INTERVAL) => {},
        }
    }
}

fn should_finish(store: &MultiAgentStore, agent_id: &JsString, signal: Option<&AbortSignal>) -> Result<bool, JsString> {
    refresh_persisted_descendant_snapshots(store, agent_id)?;
    let active = store
        .list_descendants(agent_id)
        .iter()
        .any(|agent| is_active_lifecycle(agent.lifecycle));
    let cancelling = store
        .get_agent(agent_id)
        .is_some_and(|agent| agent.lifecycle == AgentLifecycle::Cancelling);
    let can_stop_for_abort = signal.is_some_and(AbortSignal::aborted) && !cancelling;
    Ok(!active || can_stop_for_abort)
}

fn refresh_persisted_descendant_snapshots(store: &MultiAgentStore, agent_id: &JsString) -> Result<(), JsString> {
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(());
    };
    let Some(state) = read_multi_agent_state(&persistence.control_db_path, &persistence.session_path)
        .map_err(|error| error.message().to_owned())?
    else {
        return Ok(());
    };
    let agents: Vec<AgentSnapshot> = state
        .agents
        .iter()
        .map(js_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|error| JsString::from(error.to_string()))?;
    let mut children_by_parent: HashMap<Option<&JsString>, Vec<&AgentSnapshot>> = HashMap::new();
    for agent in &agents {
        children_by_parent
            .entry(agent.parent_id.as_ref())
            .or_default()
            .push(agent);
    }
    let mut pending_parent_ids = vec![agent_id];
    while let Some(parent_id) = pending_parent_ids.pop() {
        for child in children_by_parent.get(&Some(parent_id)).into_iter().flatten() {
            let newer = store
                .get_agent(&child.id)
                .is_none_or(|current| child.revision > current.revision);
            if newer {
                store
                    .publish_lifecycle_coordinator_snapshot(child)
                    .map_err(|error| error.message)?;
            }
            pending_parent_ids.push(&child.id);
        }
    }
    Ok(())
}
