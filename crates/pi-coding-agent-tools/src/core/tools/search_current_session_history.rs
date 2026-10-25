//! `search_current_session_history` tool (`core/tools/search-current-session-history.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_tui::tui::ComponentRef;
use serde::{Deserialize, Serialize};
use serde_json::Value as SchemaValue;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_manager::SessionEntry;
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
    query: JsString,
    context_entries: Option<i64>,
    limit: Option<i64>,
    cursor: Option<JsString>,
}

#[derive(Debug, Clone)]
struct SearchableSessionEntry {
    entry: SessionEntry,
    role: JsString,
    content: Value,
    search_texts: Vec<JsString>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionHistorySearchEntry {
    id: JsString,
    timestamp: JsString,
    entry_type: JsString,
    role: JsString,
    content: Value,
    matched: bool,
    compacted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchCurrentSessionHistoryDetails {
    query: JsString,
    total_matches: usize,
    returned_matches: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<JsString>,
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

pub fn parameters() -> &'static SchemaValue {
    static PARAMETERS: OnceLock<SchemaValue> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        serde_json::json!({
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

fn collect_argument_texts(value: &Value, texts: &mut Vec<JsString>) {
    match value {
        Value::String(text) => texts.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_argument_texts(item, texts)),
        Value::Object(object) => {
            for (key, item) in pi_js_compat::js_json::object_entries_lossless(object) {
                texts.push(key.clone());
                collect_argument_texts(item, texts);
            }
        }
        Value::Number(number) => texts.push(pi_js_compat::number::js_number_to_string(*number).into()),
        _ => texts.push(pi_js_compat::js_json::stringify_lossless(value).into()),
    }
}

fn collect_search_texts(content: &Value) -> Vec<JsString> {
    if let Some(text) = content.as_string() {
        return vec![text.clone()];
    }
    let mut texts = Vec::new();
    for block in content.as_array().into_iter().flatten() {
        let kind = block["type"].as_string();
        if kind == Some(&JsString::from("text")) {
            texts.push(block["text"].as_string().cloned().unwrap_or_default());
        } else if kind == Some(&JsString::from("toolCall"))
            && block["name"].as_string() != Some(&JsString::from("search_current_session_history"))
        {
            texts.push(block["name"].as_string().cloned().unwrap_or_default());
            collect_argument_texts(&block["arguments"], &mut texts);
        }
    }
    texts
}

fn collect_message_search_texts(message: &Value, content: &Value) -> Vec<JsString> {
    let role = message["role"].as_string();
    if role == Some(&JsString::from("toolResult"))
        && message["toolName"].as_string() == Some(&JsString::from("search_current_session_history"))
    {
        return Vec::new();
    }
    if role == Some(&JsString::from("bashExecution")) {
        return ["command", "output"]
            .map(|key| message[key].as_string().cloned().unwrap_or_default())
            .to_vec();
    }
    if role == Some(&JsString::from("branchSummary")) || role == Some(&JsString::from("compactionSummary")) {
        return vec![message["summary"].as_string().cloned().unwrap_or_default()];
    }
    collect_search_texts(content)
}

/// `searchableEntry()`: reads the raw stored record, as TS does, without a typed decode.
fn searchable_entry(entry: &SessionEntry) -> Option<SearchableSessionEntry> {
    let value = pi_js_compat::js_json::to_value(entry).expect("session entry must serialize");
    let kind = value["type"].as_string()?;
    let (role, content, search_texts) = if *kind == "message" {
        let message = &value["message"];
        let content = match message.as_object().and_then(|object| object.get("content")) {
            Some(content) => content.clone(),
            None => message.clone(),
        };
        let search_texts = collect_message_search_texts(message, &content);
        (
            message["role"].as_string().cloned().unwrap_or_default(),
            content,
            search_texts,
        )
    } else if *kind == "custom_message" {
        let content = value["content"].clone();
        ("custom".into(), content.clone(), collect_search_texts(&content))
    } else if *kind == "compaction" || *kind == "branch_summary" {
        let role = if *kind == "compaction" {
            "compaction"
        } else {
            "branchSummary"
        };
        let summary = value["summary"].as_string().cloned().unwrap_or_default();
        (role.into(), Value::String(summary.clone()), vec![summary])
    } else {
        return None;
    };
    Some(SearchableSessionEntry {
        entry: entry.clone(),
        role,
        content,
        search_texts,
    })
}

fn parse_cursor(cursor: Option<&JsString>) -> Result<usize, ToolError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    if cursor.is_empty()
        || !cursor
            .units()
            .iter()
            .all(|unit| (u16::from(b'0')..=u16::from(b'9')).contains(unit))
    {
        return Err(ToolError::new(
            "search_current_session_history cursor must be a non-negative integer cursor",
        ));
    }
    Ok(pi_js_compat::number::parse_int(cursor.units(), 10) as usize)
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
    if session.get_session_file().is_none() {
        return Err(ToolError::new(
            "search_current_session_history requires a persisted current session",
        ));
    }
    Ok(())
}

impl ContextTool<Option<ExtensionContext>> for SearchCurrentSessionHistoryTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &SchemaValue {
        parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        LABEL.into()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Parallel)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<AbortSignal>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: Option<ExtensionContext>,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let context = context
                .ok_or_else(|| ToolError::new("search_current_session_history requires a persisted current session"))?;
            require_persisted_session(&context)?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let manager = session;
            let params: SearchCurrentSessionHistoryParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let query = params.query.trim();
            if query.is_empty() {
                return Err(ToolError::new("search_current_session_history query must not be empty"));
            }

            let mut history = crate::core::session_history::read_session_history_reverse(
                manager.get_branch(None),
                manager.get_session_file(),
            )
            .collect::<Result<Vec<_>, _>>()
            .map_err(|cause| ToolError::new(cause.message()))?;
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
            let cursor = parse_cursor(params.cursor.as_ref())?;
            let limit = usize::try_from(params.limit.unwrap_or(DEFAULT_LIMIT as i64).max(0)).unwrap_or(0);
            let page_end = cursor.saturating_add(limit).min(all_match_indexes.len());
            let page_match_indexes: Vec<usize> = all_match_indexes
                .get(cursor.min(all_match_indexes.len())..page_end)
                .unwrap_or_default()
                .to_vec();
            let context_entries = params.context_entries.unwrap_or(0);
            let included_indexes = selected_indexes(&page_match_indexes, context_entries, searchable_entries.len());
            let match_index_set: HashSet<usize> = page_match_indexes.iter().copied().collect();
            let context_entry_ids: HashSet<JsString> = manager
                .build_context_entries()
                .iter()
                .filter_map(|entry| entry.id().cloned())
                .collect();
            let entries: Vec<SessionHistorySearchEntry> = searchable_entries
                .iter()
                .enumerate()
                .filter(|(index, _)| included_indexes.contains(index))
                .map(|(index, searchable)| {
                    let entry_id = searchable.entry.id().cloned().unwrap_or_default();
                    SessionHistorySearchEntry {
                        id: entry_id.clone(),
                        timestamp: searchable.entry.timestamp().cloned().unwrap_or_default(),
                        entry_type: searchable.entry.entry_type().to_owned(),
                        role: searchable.role.clone(),
                        content: searchable.content.clone(),
                        matched: match_index_set.contains(&index),
                        compacted: !context_entry_ids.contains(&entry_id),
                    }
                })
                .collect();
            let next_offset = cursor.saturating_add(page_match_indexes.len());
            let next_cursor = (next_offset < all_match_indexes.len()).then(|| next_offset.to_string().into());
            let details = SearchCurrentSessionHistoryDetails {
                query: query.to_owned(),
                total_matches: all_match_indexes.len(),
                returned_matches: page_match_indexes.len(),
                next_cursor,
                entries,
            };
            let details_value = pi_js_compat::js_json::to_value(&details).expect("search details must serialize");
            let text = if all_match_indexes.is_empty() {
                "No matches found in current session history.".to_owned()
            } else {
                pi_js_compat::js_json::stringify_lossless_pretty(&details_value)
            };
            Ok(text_result(text, details_value))
        })
    }
}

fn format_call(args: &JsValue, theme: &Theme) -> JsString {
    let query = match args.get("query") {
        None | Some(Value::Null) => Value::String(String::new().into()),
        Some(query) => query.clone(),
    };
    let query = pi_js_compat::js_json::stringify_lossless(&query);
    js_format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("search_current_session_history")),
        theme.fg(ThemeColor::Accent, &query)
    )
}

/// `renderCall` for the search_current_session_history tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
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
        crate::core::extensions::wrapper::context_tool_definition(Arc::new(SearchCurrentSessionHistoryTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

#[cfg(test)]
mod js_value_tests {
    use super::*;

    #[test]
    fn raw_cursor_reaches_explicit_numeric_validation_without_json_narrowing() {
        let value = pi_js_compat::js_json::parse_lossless(r#"{"query":"needle","cursor":"offset-\ud800"}"#).unwrap();
        let params: SearchCurrentSessionHistoryParams = pi_js_compat::js_json::from_value(&value).unwrap();
        let error = parse_cursor(params.cursor.as_ref()).unwrap_err();
        assert_eq!(
            error.message,
            "search_current_session_history cursor must be a non-negative integer cursor"
        );
    }

    #[test]
    fn collects_numeric_properties_in_javascript_order() {
        let value = serde_json::from_str(r#"{"2":"second","1":"first"}"#).unwrap();
        let mut texts = Vec::new();
        collect_argument_texts(&value, &mut texts);
        assert_eq!(texts, ["1", "first", "2", "second"]);
    }
}
