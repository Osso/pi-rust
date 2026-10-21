//! Lifecycle coordinator construction and snapshot publication
//! (`extensions/agents-core/src/lifecycle-runtime.ts`).

use std::sync::{Arc, OnceLock};

use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use crate::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, is_active_lifecycle};
use crate::core::runtime_process::ProcessIdentity;
use crate::core::session_control_db::get_runtime_process_instance_id;
use crate::core::sqlite::ControlDbError;
use crate::core::terminal_outbox_delivery::{DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections};

pub fn runtime_process_identity() -> &'static ProcessIdentity {
    static IDENTITY: OnceLock<ProcessIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        // TS parses this at module load, so a failure aborts startup.
        let instance_id = get_runtime_process_instance_id().expect("runtime process identity must be readable");
        serde_json::from_str(instance_id).expect("runtime process identity must be valid JSON")
    })
}

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

pub fn publish_coordinator_snapshot(store: &Arc<MultiAgentStore>, agent: &AgentSnapshot) -> Result<(), ControlDbError> {
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
        artifact_root: None,
        claim_id: pi_ai::utils::uuid::random_uuid(),
        control_db_path: persistence.control_db_path,
        now: Arc::new(crate::utils::iso_time::now_iso_string),
        store: Arc::clone(store),
    })?;
    Ok(())
}
