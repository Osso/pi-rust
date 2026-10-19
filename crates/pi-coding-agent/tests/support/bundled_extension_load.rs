//! Convenience loaders shared by extension tests that use an in-memory session.

use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{ExtensionApi, HandlerError};
use pi_coding_agent::core::session_manager::SessionManager;

use super::support::{Harness, RecordingUi};

impl Harness {
    pub async fn load(
        cwd: &str,
        ui: Arc<RecordingUi>,
        setup: impl FnOnce(&ExtensionApi) -> Result<(), HandlerError> + Send + 'static,
    ) -> Self {
        let session_manager = Arc::new(SessionManager::in_memory(Some(cwd), None).expect("in-memory session manager"));
        Self::load_with_session(cwd, session_manager, ui, setup).await
    }

    pub async fn load_with_session(
        cwd: &str,
        session_manager: Arc<SessionManager>,
        ui: Arc<RecordingUi>,
        setup: impl FnOnce(&ExtensionApi) -> Result<(), HandlerError> + Send + 'static,
    ) -> Self {
        Self::load_full(cwd, session_manager, ui, None, setup).await
    }
}
