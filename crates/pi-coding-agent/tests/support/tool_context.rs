//! Shared test doubles for first-party tool tests.

use std::sync::Arc;

use pi_coding_agent::core::session_manager::SessionEntry;
use pi_coding_agent::core::tools::context::ReadonlySessionManager;

pub struct StaticSession {
    pub cwd: String,
    pub session_dir: String,
    pub session_id: String,
    pub session_file: Option<String>,
}

impl StaticSession {
    pub fn new(cwd: &str, session_dir: &str, session_id: &str, session_file: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            cwd: cwd.to_owned(),
            session_dir: session_dir.to_owned(),
            session_id: session_id.to_owned(),
            session_file: session_file.map(str::to_owned),
        })
    }
}

impl ReadonlySessionManager for StaticSession {
    fn get_cwd(&self) -> &str {
        &self.cwd
    }

    fn get_session_dir(&self) -> &str {
        &self.session_dir
    }

    fn get_session_id(&self) -> &str {
        &self.session_id
    }

    fn get_session_file(&self) -> Option<&str> {
        self.session_file.as_deref()
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        Vec::new()
    }

    fn build_context_entries(&self) -> Vec<SessionEntry> {
        Vec::new()
    }
}
