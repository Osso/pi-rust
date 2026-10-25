//! Shared process identity used by sessions and bundled lifecycle coordination.
use std::sync::OnceLock;

use crate::core::runtime_process::ProcessIdentity;
use crate::core::session_control_db::get_runtime_process_instance_id;

pub fn runtime_process_identity() -> &'static ProcessIdentity {
    static IDENTITY: OnceLock<ProcessIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        // TS parses this at module load, so a failure aborts startup.
        let instance_id = get_runtime_process_instance_id().expect("runtime process identity must be readable");
        serde_json::from_str(instance_id).expect("runtime process identity must be valid JSON")
    })
}
