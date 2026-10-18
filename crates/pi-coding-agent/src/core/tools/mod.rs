pub mod ask_architect;
pub mod ask_questions;
pub mod ask_supervisor;
pub mod bash;
pub mod broadcast;
pub mod change_working_directory;
pub mod channel_post;
pub mod code_index;
pub mod edit;
pub mod edit_diff;
pub mod end_turn;
pub mod file_mutation_queue;
pub mod find;
pub mod grep;
pub mod list_sessions;
pub mod ls;
pub mod node_fs;
pub mod output_accumulator;
pub mod path_utils;
pub mod read;
pub mod render_utils;
pub mod result;
pub mod resume_session;
pub mod search_current_session_history;
pub mod tool_definition_wrapper;
pub mod truncate;
pub mod write;

pub use truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, format_size, truncate_head,
    truncate_line, truncate_tail,
};

/// Tools enabled for a default session (`DEFAULT_ACTIVE_TOOL_NAMES` in `tools/index.ts`).
pub const DEFAULT_ACTIVE_TOOL_NAMES: [&str; 19] = [
    "read",
    "bash",
    "edit",
    "write",
    "grep",
    "find",
    "ls",
    "outline",
    "symbol",
    "references",
    "end_turn",
    "change_working_directory",
    "resume_session",
    "search_current_session_history",
    "list_sessions",
    "broadcast",
    "channel_post",
    "ask_questions",
    "ask_supervisor",
];

/// Update the reused `Text` component when the renderer already owns one.
///
/// Matches the TS `(context.lastComponent as Text | undefined) ?? new Text("", 0, 0)`
/// pattern shared by the coordination tool renderers.
pub(crate) fn render_text_component(
    context: &crate::core::extensions::types::ToolRenderContext,
    text: String,
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
pub(crate) fn tool_result_text(result: &pi_agent_core::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.as_str()),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `\n${theme.fg(isError ? "error" : "toolOutput", output)}` or empty.
pub(crate) fn render_tool_output(
    result: &pi_agent_core::AgentToolResult,
    theme: &crate::modes::interactive::theme::Theme,
) -> String {
    let output = tool_result_text(result);
    if output.is_empty() {
        return String::new();
    }
    let color = if result.is_error == Some(true) {
        crate::modes::interactive::theme::ThemeColor::Error
    } else {
        crate::modes::interactive::theme::ThemeColor::ToolOutput
    };
    format!("\n{}", theme.fg(color, &output))
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
pub fn create_all_tool_definitions(
    cwd: &str,
    options: ToolsOptions,
) -> Vec<(&'static str, crate::core::extensions::types::ToolDefinition)> {
    vec![
        (
            "read",
            read::create_read_tool_definition(cwd, options.read.unwrap_or_default()),
        ),
        (
            "bash",
            bash::create_bash_tool_definition(cwd, options.bash.unwrap_or_default()),
        ),
        (
            "edit",
            edit::create_edit_tool_definition(cwd, options.edit.unwrap_or_default()),
        ),
        (
            "write",
            write::create_write_tool_definition(cwd, options.write.unwrap_or_default()),
        ),
        (
            "grep",
            grep::create_grep_tool_definition(cwd, options.grep.unwrap_or_default()),
        ),
        (
            "find",
            find::create_find_tool_definition(cwd, options.find.unwrap_or_default()),
        ),
        ("ls", ls::create_ls_tool_definition(cwd, options.ls.unwrap_or_default())),
        (
            "outline",
            code_index::create_outline_tool_definition(cwd, options.code_index.clone().unwrap_or_default()),
        ),
        (
            "symbol",
            code_index::create_symbol_tool_definition(cwd, options.code_index.clone().unwrap_or_default()),
        ),
        (
            "references",
            code_index::create_references_tool_definition(cwd, options.code_index.unwrap_or_default()),
        ),
        ("end_turn", end_turn::create_end_turn_tool_definition()),
        (
            "change_working_directory",
            change_working_directory::create_change_working_directory_tool_definition(),
        ),
        (
            "resume_session",
            resume_session::create_resume_session_tool_definition(),
        ),
        (
            "search_current_session_history",
            search_current_session_history::create_search_current_session_history_tool_definition(),
        ),
        ("list_sessions", list_sessions::create_list_sessions_tool_definition()),
        ("broadcast", broadcast::create_broadcast_tool_definition()),
        ("channel_post", channel_post::create_channel_post_tool_definition()),
        ("ask_questions", ask_questions::create_ask_questions_tool_definition()),
        (
            "ask_supervisor",
            ask_supervisor::create_ask_supervisor_tool_definition(),
        ),
    ]
}
