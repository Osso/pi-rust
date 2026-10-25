//! Lifecycle coordinator construction and snapshot publication
//! (`extensions/agents-core/src/lifecycle-runtime.ts`).

use std::sync::Arc;

use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use crate::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, is_active_lifecycle};
use crate::core::sqlite::ControlDbError;
use crate::core::terminal_outbox_delivery::{DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections};

pub use crate::core::runtime_process_identity::runtime_process_identity;

pub fn create_lifecycle_coordinator(store: &Arc<MultiAgentStore>) -> Option<Arc<LifecycleCoordinator>> {
    let persistence = store.get_persistence_target()?;
    let store = Arc::clone(store);
    Some(Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: persistence.control_db_path,
        create_agent_id: Arc::new(move || {
            store
                .allocate_agent_id_for_lifecycle_coordinator("agent")
                .map_err(|error| crate::core::sqlite::ControlDbError::new(error.message))
        }),
        now: Arc::new(crate::utils::iso_time::now_iso_string),
        process_identity: runtime_process_identity().clone(),
        session_path: persistence.session_path,
    })))
}

pub fn publish_coordinator_snapshot(
    store: &Arc<MultiAgentStore>,
    agent: &AgentSnapshot,
) -> Result<(), pi_js_compat::callback_error::CallbackError> {
    store
        .publish_lifecycle_coordinator_snapshot(agent)
        .map_err(|error| ControlDbError::new(error.message))?;
    if is_active_lifecycle(agent.lifecycle) {
        return Ok(());
    }
    let Some(persistence) = store.get_persistence_target() else {
        return Ok(());
    };
    deliver_terminal_outbox_projections(&DeliverTerminalOutboxOptions {
        artifact_root: super::runtime::artifact_root_for_store(store),
        claim_id: pi_ai::utils::random_uuid::random_uuid()?.into(),
        control_db_path: persistence.control_db_path,
        now: Arc::new(|| Ok(crate::utils::iso_time::now_iso_string().into())),
        store: Arc::clone(store),
    })?;
    Ok(())
}
