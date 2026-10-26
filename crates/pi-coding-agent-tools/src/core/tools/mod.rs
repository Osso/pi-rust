use pi_coding_agent_tool_support::core::tools::abortable;
pub mod ask_architect;
pub mod ask_questions;
pub mod ask_questions_panel;
pub mod ask_supervisor;
pub mod bash;
pub mod change_working_directory;
pub mod channel_post;
pub mod code_index;
pub mod edit;
pub use pi_coding_agent_tool_support::core::tools::edit_diff;
pub use pi_coding_agent_tool_support::core::tools::edit_primitives;
pub mod end_turn;
pub use pi_coding_agent_tool_support::core::tools::file_mutation_queue;
pub mod find;
pub mod grep;
pub use pi_coding_agent_tool_support::core::tools::jsdiff;
pub mod list_sessions;
pub mod ls;
pub use pi_coding_agent_tool_support::core::tools::node_fs;
pub use pi_coding_agent_tool_support::core::tools::output_accumulator;
pub use pi_coding_agent_tool_support::core::tools::path_utils;
pub mod read;
pub mod render_utils;
pub use pi_coding_agent_tool_support::core::tools::result;
pub mod resume_session;
pub mod search_current_session_history;
pub mod tool_definition_wrapper;
pub use pi_coding_agent_tool_support::core::tools::truncate;
pub use pi_coding_agent_tool_support::core::tools::unicode;
pub mod write;

pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line, truncate_tail,
};

/// System-prompt contribution exported by a built-in tool (`*ToolSystemPromptContribution`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSystemPromptContribution {
    pub snippet: &'static str,
    pub guidelines: &'static [&'static str],
}

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

use crate::core::extensions::types::ToolDefinition;
use crate::core::tools::tool_definition_wrapper::wrap_definition;

/// Tool names exported by `tools/index.ts` (`ToolName`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolName {
    Read,
    Bash,
    Edit,
    Write,
    Grep,
    Find,
    Ls,
    Outline,
    Symbol,
    References,
    EndTurn,
    ChangeWorkingDirectory,
    ResumeSession,
    SearchCurrentSessionHistory,
    ListSessions,
    ChannelPost,
    AskQuestions,
    AskSupervisor,
}

impl ToolName {
    /// `allToolNames` / `DEFAULT_ACTIVE_TOOL_NAMES` in source order.
    pub const ALL: [ToolName; 18] = [
        ToolName::Read,
        ToolName::Bash,
        ToolName::Edit,
        ToolName::Write,
        ToolName::Grep,
        ToolName::Find,
        ToolName::Ls,
        ToolName::Outline,
        ToolName::Symbol,
        ToolName::References,
        ToolName::EndTurn,
        ToolName::ChangeWorkingDirectory,
        ToolName::ResumeSession,
        ToolName::SearchCurrentSessionHistory,
        ToolName::ListSessions,
        ToolName::ChannelPost,
        ToolName::AskQuestions,
        ToolName::AskSupervisor,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ToolName::Read => "read",
            ToolName::Bash => "bash",
            ToolName::Edit => "edit",
            ToolName::Write => "write",
            ToolName::Grep => "grep",
            ToolName::Find => "find",
            ToolName::Ls => "ls",
            ToolName::Outline => "outline",
            ToolName::Symbol => "symbol",
            ToolName::References => "references",
            ToolName::EndTurn => "end_turn",
            ToolName::ChangeWorkingDirectory => "change_working_directory",
            ToolName::ResumeSession => "resume_session",
            ToolName::SearchCurrentSessionHistory => "search_current_session_history",
            ToolName::ListSessions => "list_sessions",
            ToolName::ChannelPost => "channel_post",
            ToolName::AskQuestions => "ask_questions",
            ToolName::AskSupervisor => "ask_supervisor",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.as_str() == name)
    }
}

impl std::fmt::Display for ToolName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Tools enabled for a default session (`DEFAULT_ACTIVE_TOOL_NAMES` in `tools/index.ts`).
pub const DEFAULT_ACTIVE_TOOL_NAMES: [ToolName; 18] = ToolName::ALL;

/// `allToolNames`.
pub const ALL_TOOL_NAMES: [ToolName; 18] = ToolName::ALL;

/// Update the reused `Text` component when the renderer already owns one.
///
/// Matches the TS `(context.lastComponent as Text | undefined) ?? new Text("", 0, 0)`
/// pattern shared by the coordination tool renderers.
pub(crate) fn render_text_component(
    context: &crate::core::extensions::types::ToolRenderContext,
    text: JsString,
) -> pi_tui::tui::ComponentRef {
    if let Some(component) = &context.last_component
        && let Some(text_component) = component
            .borrow_mut()
            .as_any_mut()
            .and_then(|component| component.downcast_mut::<pi_tui::Text>())
    {
        text_component.set_text(text);
        return std::rc::Rc::clone(component);
    }
    std::rc::Rc::new(std::cell::RefCell::new(pi_tui::Text::new(text, 0, 0)))
}

/// Join text content blocks with newlines, ignoring images.
pub(crate) fn tool_result_text(result: &pi_agent_core::AgentToolResult) -> JsString {
    pi_js_compat::js_json::JsString::join(
        result.content.iter().filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        }),
        "\n",
    )
}

/// `\n${theme.fg(isError ? "error" : "toolOutput", output)}` or empty.
pub(crate) fn render_tool_output(
    result: &pi_agent_core::AgentToolResult,
    theme: &crate::modes::interactive::theme::Theme,
) -> JsString {
    let output = tool_result_text(result);
    if output.is_empty() {
        return JsString::default();
    }
    let color = if result.is_error == Some(true) {
        crate::modes::interactive::theme::ThemeColor::Error
    } else {
        crate::modes::interactive::theme::ThemeColor::ToolOutput
    };
    js_format!("\n{}", theme.fg(color, &output))
}

/// Per-tool option overrides (`ToolsOptions` in `tools/index.ts`).
#[derive(Clone, Default)]
pub struct ToolsOptions {
    pub read: Option<read::ReadToolOptions>,
    pub bash: Option<bash::BashToolOptions>,
    pub write: Option<write::WriteToolOptions>,
    pub edit: Option<edit::EditToolOptions>,
    pub grep: Option<grep::GrepToolOptions>,
    pub find: Option<find::FindToolOptions>,
    pub ls: Option<ls::LsToolOptions>,
    pub code_index: Option<code_index::CodeIndexToolOptions>,
}

/// `createAllToolDefinitions`: every built-in definition in source order.
pub fn create_all_tool_definitions(cwd: &JsString, options: ToolsOptions) -> Vec<(ToolName, ToolDefinition)> {
    ToolName::ALL
        .into_iter()
        .map(|name| (name, create_tool_definition(name, cwd, options.clone())))
        .collect()
}

/// `createToolDefinitionsForAvailableExecutables`: hide tools whose executable is absent.
pub fn create_tool_definitions_for_available_executables(
    cwd: &JsString,
    options: ToolsOptions,
) -> Result<Vec<(ToolName, ToolDefinition)>, Box<pi_js_compat::node_error::NativeNodeError>> {
    let definitions = create_all_tool_definitions(cwd, options.clone());
    let injected = options
        .code_index
        .as_ref()
        .is_some_and(|options| options.operations.is_some());
    if injected || crate::utils::executable::is_executable_available(&"code-index".into(), &Default::default())? {
        return Ok(definitions);
    }
    Ok(definitions
        .into_iter()
        .filter(|(name, _)| !matches!(name, ToolName::Outline | ToolName::Symbol | ToolName::References))
        .collect())
}

/// `createToolDefinition`: one built-in definition by name.
pub fn create_tool_definition(name: ToolName, cwd: &JsString, options: ToolsOptions) -> ToolDefinition {
    match name {
        ToolName::Read => read::create_read_tool_definition(cwd, options.read.unwrap_or_default()),
        ToolName::Bash => bash::create_bash_tool_definition(cwd, options.bash.unwrap_or_default()),
        ToolName::Edit => edit::create_edit_tool_definition(cwd, options.edit.unwrap_or_default()),
        ToolName::Write => write::create_write_tool_definition(cwd, options.write.unwrap_or_default()),
        ToolName::Grep => grep::create_grep_tool_definition(cwd, options.grep.unwrap_or_default()),
        ToolName::Find => find::create_find_tool_definition(cwd, options.find.unwrap_or_default()),
        ToolName::Ls => ls::create_ls_tool_definition(cwd, options.ls.unwrap_or_default()),
        ToolName::Outline => {
            code_index::create_outline_tool_definition(cwd, options.code_index.clone().unwrap_or_default())
        }
        ToolName::Symbol => {
            code_index::create_symbol_tool_definition(cwd, options.code_index.clone().unwrap_or_default())
        }
        ToolName::References => {
            code_index::create_references_tool_definition(cwd, options.code_index.unwrap_or_default())
        }
        ToolName::EndTurn => end_turn::create_end_turn_tool_definition(),
        ToolName::ChangeWorkingDirectory => change_working_directory::create_change_working_directory_tool_definition(),
        ToolName::ResumeSession => resume_session::create_resume_session_tool_definition(),
        ToolName::SearchCurrentSessionHistory => {
            search_current_session_history::create_search_current_session_history_tool_definition()
        }
        ToolName::ListSessions => list_sessions::create_list_sessions_tool_definition(),
        ToolName::ChannelPost => channel_post::create_channel_post_tool_definition(),
        ToolName::AskQuestions => ask_questions::create_ask_questions_tool_definition(),
        ToolName::AskSupervisor => ask_supervisor::create_ask_supervisor_tool_definition(),
    }
}

/// `createCodingToolDefinitions`.
pub fn create_coding_tool_definitions(cwd: &JsString, options: ToolsOptions) -> Vec<ToolDefinition> {
    [ToolName::Read, ToolName::Bash, ToolName::Edit, ToolName::Write]
        .into_iter()
        .map(|name| create_tool_definition(name, cwd, options.clone()))
        .collect()
}

/// `createReadOnlyToolDefinitions`.
pub fn create_read_only_tool_definitions(cwd: &JsString, options: ToolsOptions) -> Vec<ToolDefinition> {
    [
        ToolName::Read,
        ToolName::Grep,
        ToolName::Find,
        ToolName::Ls,
        ToolName::Outline,
        ToolName::Symbol,
        ToolName::References,
    ]
    .into_iter()
    .map(|name| create_tool_definition(name, cwd, options.clone()))
    .collect()
}

/// `createTool`: one built-in tool by name.
///
/// Standalone factories preserve the source's undefined extension context.
pub fn create_tool(name: ToolName, cwd: &JsString, options: ToolsOptions) -> Arc<dyn AgentTool> {
    match name {
        ToolName::Read => read::create_read_tool(cwd, options.read.unwrap_or_default()),
        ToolName::Bash => bash::create_bash_tool(cwd, options.bash.unwrap_or_default()),
        ToolName::Edit => edit::create_edit_tool(cwd, options.edit.unwrap_or_default()),
        ToolName::Write => write::create_write_tool(cwd, options.write.unwrap_or_default()),
        ToolName::Grep => grep::create_grep_tool(cwd, options.grep.unwrap_or_default()),
        ToolName::Find => find::create_find_tool(cwd, options.find.unwrap_or_default()),
        ToolName::Ls => ls::create_ls_tool(cwd, options.ls.unwrap_or_default()),
        ToolName::Outline => code_index::create_outline_tool(cwd, options.code_index.clone().unwrap_or_default()),
        ToolName::Symbol => code_index::create_symbol_tool(cwd, options.code_index.clone().unwrap_or_default()),
        ToolName::References => code_index::create_references_tool(cwd, options.code_index.unwrap_or_default()),
        ToolName::EndTurn
        | ToolName::ChangeWorkingDirectory
        | ToolName::ResumeSession
        | ToolName::SearchCurrentSessionHistory
        | ToolName::ListSessions
        | ToolName::ChannelPost
        | ToolName::AskQuestions
        | ToolName::AskSupervisor => wrap_definition(create_tool_definition(name, cwd, options), None),
    }
}

/// `createCodingTools`.
pub fn create_coding_tools(cwd: &JsString, options: ToolsOptions) -> Vec<Arc<dyn AgentTool>> {
    [ToolName::Read, ToolName::Bash, ToolName::Edit, ToolName::Write]
        .into_iter()
        .map(|name| create_tool(name, cwd, options.clone()))
        .collect()
}

/// `createReadOnlyTools`.
pub fn create_read_only_tools(cwd: &JsString, options: ToolsOptions) -> Vec<Arc<dyn AgentTool>> {
    [
        ToolName::Read,
        ToolName::Grep,
        ToolName::Find,
        ToolName::Ls,
        ToolName::Outline,
        ToolName::Symbol,
        ToolName::References,
    ]
    .into_iter()
    .map(|name| create_tool(name, cwd, options.clone()))
    .collect()
}

/// `createAllTools`: every built-in tool in source order.
pub fn create_all_tools(cwd: &JsString, options: ToolsOptions) -> Vec<(ToolName, Arc<dyn AgentTool>)> {
    ToolName::ALL
        .into_iter()
        .map(|name| (name, create_tool(name, cwd, options.clone())))
        .collect()
}
