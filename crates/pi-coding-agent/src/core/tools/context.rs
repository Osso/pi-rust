//! Execution context for first-party tools.
//!
//! `core/extensions/types.ts` is not ported yet. This module carries exactly the
//! subset of `ExtensionContext` the first-party session/coordination tools read,
//! plus the injected collaborators those tools use instead of importing the
//! interactive runtime.

use std::sync::Arc;

use pi_agent_core::ToolError;
use pi_ai::request::BoxFuture;

use crate::core::desktop_notification::DesktopNotifier;
use crate::core::session_manager::{SessionEntry, SessionManager};

/// `ExtensionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionMode {
    Tui,
    Rpc,
    Json,
    Print,
}

impl ExtensionMode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Rpc => "rpc",
            Self::Json => "json",
            Self::Print => "print",
        }
    }
}

/// The `ReadonlySessionManager` surface these tools use.
pub trait ReadonlySessionManager: Send + Sync {
    fn get_cwd(&self) -> &str;
    fn get_session_dir(&self) -> &str;
    fn get_session_id(&self) -> &str;
    fn get_session_file(&self) -> Option<&str>;
    fn get_branch(&self) -> Vec<SessionEntry>;
    fn build_context_entries(&self) -> Vec<SessionEntry>;
}

impl ReadonlySessionManager for SessionManager {
    fn get_cwd(&self) -> &str {
        SessionManager::get_cwd(self)
    }

    fn get_session_dir(&self) -> &str {
        SessionManager::get_session_dir(self)
    }

    fn get_session_id(&self) -> &str {
        SessionManager::get_session_id(self)
    }

    fn get_session_file(&self) -> Option<&str> {
        SessionManager::get_session_file(self)
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        SessionManager::get_branch(self, None)
    }

    fn build_context_entries(&self) -> Vec<SessionEntry> {
        SessionManager::build_context_entries(self)
    }
}

/// `ctx.ui.select` / `ctx.ui.input` used by `ask_questions`.
pub trait ToolUi: Send + Sync {
    fn select<'a>(&'a self, title: &'a str, options: &'a [String]) -> BoxFuture<'a, Option<String>>;
    fn input<'a>(&'a self, title: &'a str, placeholder: Option<&'a str>) -> BoxFuture<'a, Option<String>>;
}

/// Result of `ctx.switchSession`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchSessionResult {
    pub cancelled: bool,
}

/// `ctx.switchSession(sessionPath, { withSession })`.
///
/// `starter_prompt` carries the only `withSession` callback in the TypeScript
/// tools: it sends the prompt after the target session is active.
pub trait SessionSwitcher: Send + Sync {
    fn switch_session<'a>(
        &'a self,
        session_path: &'a str,
        starter_prompt: Option<&'a str>,
    ) -> BoxFuture<'a, Result<SwitchSessionResult, ToolError>>;
}

/// `ctx.relocateAfterToolResult`.
pub trait RelocateAfterToolResult: Send + Sync {
    fn relocate_after_tool_result(&self, tool_call_id: &str, target_cwd: &str);
}

/// Snapshot of the extension context for one tool call.
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: String,
    pub control_db_path: Option<String>,
    pub mode: ExtensionMode,
    pub has_ui: bool,
    pub multi_agent_agent_id: Option<String>,
    pub multi_agent_requires_agent_id: bool,
    pub session: Arc<dyn ReadonlySessionManager>,
    pub ui: Option<Arc<dyn ToolUi>>,
    pub switch_session: Option<Arc<dyn SessionSwitcher>>,
    pub relocate_after_tool_result: Option<Arc<dyn RelocateAfterToolResult>>,
    pub desktop_notifier: Option<Arc<dyn DesktopNotifier>>,
}

impl ToolContext {
    pub fn new(cwd: impl Into<String>, session: Arc<dyn ReadonlySessionManager>) -> Self {
        Self {
            cwd: cwd.into(),
            control_db_path: None,
            mode: ExtensionMode::Print,
            has_ui: false,
            multi_agent_agent_id: None,
            multi_agent_requires_agent_id: false,
            session,
            ui: None,
            switch_session: None,
            relocate_after_tool_result: None,
            desktop_notifier: None,
        }
    }

    pub fn with_control_db_path(mut self, control_db_path: impl Into<String>) -> Self {
        self.control_db_path = Some(control_db_path.into());
        self
    }

    pub fn with_mode(mut self, mode: ExtensionMode, has_ui: bool) -> Self {
        self.mode = mode;
        self.has_ui = has_ui;
        self
    }

    pub fn with_ui(mut self, ui: Arc<dyn ToolUi>) -> Self {
        self.ui = Some(ui);
        self.has_ui = true;
        self
    }

    pub fn with_switch_session(mut self, switcher: Arc<dyn SessionSwitcher>) -> Self {
        self.switch_session = Some(switcher);
        self
    }

    pub fn with_relocate_after_tool_result(mut self, relocate: Arc<dyn RelocateAfterToolResult>) -> Self {
        self.relocate_after_tool_result = Some(relocate);
        self
    }

    pub fn with_desktop_notifier(mut self, notifier: Arc<dyn DesktopNotifier>) -> Self {
        self.desktop_notifier = Some(notifier);
        self
    }

    pub fn with_multi_agent_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.multi_agent_agent_id = Some(agent_id.into());
        self
    }

    pub fn require_control_db_path(&self, tool_name: &str) -> Result<&str, ToolError> {
        self.control_db_path
            .as_deref()
            .ok_or_else(|| ToolError::new(format!("{tool_name} requires a control database path")))
    }
}
