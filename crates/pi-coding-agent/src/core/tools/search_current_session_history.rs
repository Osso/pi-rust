//! `search_current_session_history` tool (`core/tools/search-current-session-history.ts`).

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use pi_agent_core::{
    AgentHarnessTool, AgentMessage, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError,
    ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use pi_tui::tui::ComponentRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_manager::{SessionEntry, lock_session_manager};
use crate::modes::interactive::theme::{Theme, ThemeColor};

const DEFAULT_LIMIT: usize = 20;

pub const NAME: &str = "search_current_session_history";
pub const LABEL: &str = "search_current_session_history";
pub const DESCRIPTION: &str = "Search conversational content in the current persisted session's active branch, including full entries hidden from model context by compaction.";
pub const PROMPT_SNIPPET: &str = "Search the current stored session history after compaction";
pub const PROMPT_GUIDELINES: [&str; 2] = [
    "Use search_current_session_history to recover details from the current session that may have been omitted by compaction.",
    "Matching includes user and assistant text, tool-call names and arguments, tool results, custom messages, and summaries; excludes thinking, signatures, and this tool's own calls and results.",
];

#[derive(Debug, Deserialize)]
struct SearchCurrentSessionHistoryParams {
    query: String,
    context_entries: Option<i64>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Debug, Clone)]
struct SearchableSessionEntry {
    entry: SessionEntry,
    role: String,
    content: Value,
    search_texts: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionHistorySearchEntry {
    id: String,
    timestamp: String,
    entry_type: String,
    role: String,
    content: Value,
    matched: bool,
    compacted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchCurrentSessionHistoryDetails {
    query: String,
    total_matches: usize,
    returned_matches: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
    entries: Vec<SessionHistorySearchEntry>,
}

/// `createSearchCurrentSessionHistoryToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SearchCurrentSessionHistoryTool;

impl SearchCurrentSessionHistoryTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static Value {
    static PARAMETERS: OnceLock<Value> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Case-insensitive literal text to search for."
                },
                "context_entries": {
                    "type": "integer",
                    "description": "Number of neighboring searchable entries to include before and after each match. Defaults to 0.",
                    "minimum": 0,
                    "maximum": 10
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum matching entries to return. Defaults to 20.",
                    "minimum": 1,
                    "maximum": 50
                },
                "cursor": {
                    "type": "string",
                    "description": "Pagination cursor returned by a previous search."
                }
            },
            "required": ["query"]
        })
    })
}

fn collect_argument_texts(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => texts.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_argument_texts(item, texts)),
        Value::Object(object) => {
            for (key, item) in pi_js_compat::js_json::object_entries(object) {
                texts.push(key.to_owned());
                collect_argument_texts(item, texts);
            }
        }
        Value::Number(number) => texts.push(pi_js_compat::number::js_number_to_string(
            number.as_f64().expect("JSON number fits binary64"),
        )),
        _ => texts.push(value.to_string()),
    }
}

fn collect_search_texts(content: &Value) -> Vec<String> {
    if let Some(text) = content.as_str() {
        return vec![text.to_owned()];
    }
    let mut texts = Vec::new();
    for block in content.as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") => texts.push(block["text"].as_str().unwrap_or_default().to_owned()),
            Some("toolCall") if block["name"] != "search_current_session_history" => {
                texts.push(block["name"].as_str().unwrap_or_default().to_owned());
                collect_argument_texts(&block["arguments"], &mut texts);
            }
            _ => {}
        }
    }
    texts
}

fn collect_message_search_texts(message: &AgentMessage, content: &Value) -> Vec<String> {
    let value = serde_json::to_value(message).expect("agent message must serialize");
    match message.role() {
        "toolResult" if value["toolName"] == "search_current_session_history" => Vec::new(),
        "bashExecution" => ["command", "output"]
            .map(|key| value[key].as_str().unwrap_or_default().to_owned())
            .to_vec(),
        "branchSummary" | "compactionSummary" => vec![value["summary"].as_str().unwrap_or_default().to_owned()],
        _ => collect_search_texts(content),
    }
}

fn message_content(message: &AgentMessage) -> Value {
    let value = serde_json::to_value(message).expect("agent message must serialize");
    match &value {
        Value::Object(object) => object.get("content").cloned().unwrap_or_else(|| value.clone()),
        _ => value,
    }
}

fn searchable_entry(entry: &SessionEntry) -> Option<SearchableSessionEntry> {
    match entry {
        SessionEntry::Known(known) => match known {
            crate::core::session_manager::KnownSessionEntry::Message(message) => {
                let role = message.message.role().to_owned();
                let content = message_content(&message.message);
                let search_texts = collect_message_search_texts(&message.message, &content);
                Some(SearchableSessionEntry {
                    entry: entry.clone(),
                    role,
                    content,
                    search_texts,
                })
            }
            crate::core::session_manager::KnownSessionEntry::CustomMessage(custom_message) => {
                let value = serde_json::to_value(custom_message).expect("custom message must serialize");
                let content = value
                    .as_object()
                    .and_then(|object| object.get("content"))
                    .cloned()
                    .unwrap_or(Value::Null);
                Some(SearchableSessionEntry {
                    entry: entry.clone(),
                    role: "custom".to_owned(),
                    search_texts: collect_search_texts(&content),
                    content,
                })
            }
            crate::core::session_manager::KnownSessionEntry::Compaction(compaction) => Some(SearchableSessionEntry {
                entry: entry.clone(),
                role: "compaction".to_owned(),
                content: Value::String(compaction.summary.clone()),
                search_texts: vec![compaction.summary.clone()],
            }),
            crate::core::session_manager::KnownSessionEntry::BranchSummary(summary) => Some(SearchableSessionEntry {
                entry: entry.clone(),
                role: "branchSummary".to_owned(),
                content: Value::String(summary.summary.clone()),
                search_texts: vec![summary.summary.clone()],
            }),
            _ => None,
        },
        SessionEntry::Unknown(_) => None,
    }
}

fn parse_cursor(cursor: Option<&str>) -> Result<usize, ToolError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    if cursor.is_empty() || !cursor.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ToolError::new(
            "search_current_session_history cursor must be a non-negative integer cursor",
        ));
    }
    Ok(cursor.parse::<f64>().map(|value| value as usize).unwrap_or(usize::MAX))
}

fn selected_indexes(match_indexes: &[usize], context_entries: i64, entry_count: usize) -> HashSet<usize> {
    let mut indexes = HashSet::new();
    let context_entries = usize::try_from(context_entries.max(0)).unwrap_or(usize::MAX);
    for &match_index in match_indexes {
        let first_index = match_index.saturating_sub(context_entries);
        let last_index = (match_index + context_entries).min(entry_count.saturating_sub(1));
        for index in first_index..=last_index {
            indexes.insert(index);
        }
    }
    indexes
}

fn require_persisted_session(context: &ExtensionContext) -> Result<(), ToolError> {
    let session = context
        .session_manager()
        .map_err(|error| ToolError::new(error.message))?;
    if lock_session_manager(&session).get_session_file().is_none() {
        return Err(ToolError::new(
            "search_current_session_history requires a persisted current session",
        ));
    }
    Ok(())
}

impl AgentHarnessTool<ExtensionContext> for SearchCurrentSessionHistoryTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> &str {
        LABEL
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Parallel)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            require_persisted_session(&context)?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let manager = lock_session_manager(&session);
            let params: SearchCurrentSessionHistoryParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let query = js_trim(&params.query);
            if query.is_empty() {
                return Err(ToolError::new("search_current_session_history query must not be empty"));
            }

            let mut history = crate::core::session_history::read_session_history_reverse(&manager)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|cause| ToolError::new(cause.to_string()))?;
            history.reverse();
            let searchable_entries: Vec<SearchableSessionEntry> = history.iter().filter_map(searchable_entry).collect();
            let normalized_query = query.to_lowercase();
            let all_match_indexes: Vec<usize> = searchable_entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    entry
                        .search_texts
                        .iter()
                        .any(|text| text.to_lowercase().contains(&normalized_query))
                })
                .map(|(index, _)| index)
                .collect();
            let cursor = parse_cursor(params.cursor.as_deref())?;
            let limit = usize::try_from(params.limit.unwrap_or(DEFAULT_LIMIT as i64).max(0)).unwrap_or(0);
            let page_end = cursor.saturating_add(limit).min(all_match_indexes.len());
            let page_match_indexes: Vec<usize> = all_match_indexes
                .get(cursor.min(all_match_indexes.len())..page_end)
                .unwrap_or_default()
                .to_vec();
            let context_entries = params.context_entries.unwrap_or(0);
            let included_indexes = selected_indexes(&page_match_indexes, context_entries, searchable_entries.len());
            let match_index_set: HashSet<usize> = page_match_indexes.iter().copied().collect();
            let context_entry_ids: HashSet<String> = manager
                .build_context_entries()
                .iter()
                .filter_map(|entry| entry.id().map(str::to_owned))
                .collect();
            let entries: Vec<SessionHistorySearchEntry> = searchable_entries
                .iter()
                .enumerate()
                .filter(|(index, _)| included_indexes.contains(index))
                .map(|(index, searchable)| {
                    let entry_id = searchable.entry.id().unwrap_or_default();
                    SessionHistorySearchEntry {
                        id: entry_id.to_owned(),
                        timestamp: searchable.entry.timestamp().unwrap_or_default().to_owned(),
                        entry_type: searchable.entry.entry_type().to_owned(),
                        role: searchable.role.clone(),
                        content: searchable.content.clone(),
                        matched: match_index_set.contains(&index),
                        compacted: !context_entry_ids.contains(entry_id),
                    }
                })
                .collect();
            let next_offset = cursor.saturating_add(page_match_indexes.len());
            let next_cursor = (next_offset < all_match_indexes.len()).then(|| next_offset.to_string());
            let details = SearchCurrentSessionHistoryDetails {
                query: query.to_owned(),
                total_matches: all_match_indexes.len(),
                returned_matches: page_match_indexes.len(),
                next_cursor,
                entries,
            };
            let details_value = serde_json::to_value(&details).expect("search details must serialize");
            let text = if all_match_indexes.is_empty() {
                "No matches found in current session history.".to_owned()
            } else {
                pi_js_compat::js_json::stringify_pretty(&details_value)
            };
            Ok(text_result(text, details_value))
        })
    }
}

fn format_call(args: &Value, theme: &Theme) -> String {
    let query = match args.get("query") {
        None | Some(Value::Null) => Value::String(String::new()),
        Some(query) => query.clone(),
    };
    let query = pi_js_compat::js_json::stringify(&query);
    format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("search_current_session_history")),
        theme.fg(ThemeColor::Accent, &query)
    )
}

/// `renderCall` for the search_current_session_history tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_call(args, theme))
}

/// `renderResult` for the search_current_session_history tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `createSearchCurrentSessionHistoryToolDefinition()`.
pub fn create_search_current_session_history_tool_definition() -> ToolDefinition {
    let mut definition =
        crate::core::extensions::wrapper::harness_tool_definition(Arc::new(SearchCurrentSessionHistoryTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

#[cfg(test)]
mod js_value_tests {
    use super::*;

    #[test]
    fn collects_numeric_properties_in_javascript_order() {
        let value = serde_json::from_str(r#"{"2":"second","1":"first"}"#).unwrap();
        let mut texts = Vec::new();
        collect_argument_texts(&value, &mut texts);
        assert_eq!(texts, ["1", "first", "2", "second"]);
    }
}
