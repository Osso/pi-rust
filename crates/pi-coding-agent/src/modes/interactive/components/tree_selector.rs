//! Ported from `coding-agent/src/modes/interactive/components/tree-selector.ts`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextOrImageContent, UserContent};
use pi_js_compat::date::{date_parse_millis, local_date_fields};
use pi_js_compat::string::{js_trim, utf16_len};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::{KeyId, get_keybindings};
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};
use pi_tui::utils::{slice_by_column, truncate_to_width_with, visible_width, wrap_text_with_ansi};
use regex::Regex;
use serde_json::Value;

use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{format_key_text, key_hint};
use crate::core::messages::CustomMessageContent;
use crate::core::session_manager::{
    KnownSessionEntry, LeafSelection, SessionEntry, SessionTreeNode, build_context_entries,
};
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, with_theme};

/// Called with the selected entry's copy text.
pub type CopyHandler = Rc<dyn Fn(Option<String>)>;
/// Called with `(entryId, label)` when a label is edited.
pub type LabelEditHandler = Rc<dyn Fn(&str, Option<&str>)>;

const TREE_GUTTER_WIDTH: usize = 2;
const MIN_VISIBLE_ANCHOR_CONTENT_WIDTH: usize = 4;
const MAX_VISIBLE_ANCHOR_CONTENT_WIDTH: usize = 20;
const MIN_ANCHOR_CONTEXT_WIDTH: usize = 2;
const MAX_ANCHOR_CONTEXT_WIDTH: usize = 12;

fn truncate(text: &str, width: usize, ellipsis: &str) -> String {
    truncate_to_width_with(text, width, ellipsis, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GutterInfo {
    pub position: usize,
    pub show: bool,
}

#[derive(Clone, Debug)]
pub struct FlatNode {
    pub node: SessionTreeNode,
    pub indent: usize,
    pub show_connector: bool,
    pub is_last: bool,
    pub gutters: Vec<GutterInfo>,
    pub is_virtual_root_child: bool,
}

/// Filter mode for tree display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Default,
    NoTools,
    UserOnly,
    LabeledOnly,
    All,
}

impl FilterMode {
    const ALL_MODES: [FilterMode; 5] = [
        FilterMode::Default,
        FilterMode::NoTools,
        FilterMode::UserOnly,
        FilterMode::LabeledOnly,
        FilterMode::All,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::NoTools => " [no-tools]",
            Self::UserOnly => " [user]",
            Self::LabeledOnly => " [labeled]",
            Self::All => " [all]",
        }
    }
}

#[derive(Clone, Debug)]
struct ToolCallInfo {
    name: String,
    arguments: Value,
}

struct HorizontalViewportRow {
    gutter: String,
    body: String,
    anchor_col: usize,
    body_width: usize,
    is_selected: bool,
}

fn render_horizontal_viewport(rows: &[HorizontalViewportRow], width: usize) -> Vec<String> {
    let viewport_width = width.saturating_sub(TREE_GUTTER_WIDTH);
    let max_body_width = rows.iter().map(|row| row.body_width).max().unwrap_or(0);
    let max_horizontal_scroll = max_body_width.saturating_sub(viewport_width);
    let selected_row = rows.iter().find(|row| row.is_selected);

    let mut horizontal_scroll = 0;
    if let Some(selected_row) = selected_row
        && max_horizontal_scroll > 0
    {
        let min_visible_anchor_content_width =
            MAX_VISIBLE_ANCHOR_CONTENT_WIDTH.min(MIN_VISIBLE_ANCHOR_CONTENT_WIDTH.max(viewport_width / 3));
        if selected_row.anchor_col > viewport_width.saturating_sub(min_visible_anchor_content_width) {
            let anchor_context_width = MAX_ANCHOR_CONTEXT_WIDTH.min(MIN_ANCHOR_CONTEXT_WIDTH.max(viewport_width / 4));
            horizontal_scroll = max_horizontal_scroll.min(selected_row.anchor_col.saturating_sub(anchor_context_width));
        }
    }

    rows.iter()
        .map(|row| {
            let line = if horizontal_scroll > 0 {
                format!(
                    "{}{}\u{1b}[0m",
                    row.gutter,
                    slice_by_column(&row.body, horizontal_scroll, viewport_width, true)
                )
            } else {
                format!("{}{}", row.gutter, row.body)
            };
            truncate(&line, width, "")
        })
        .collect()
}

fn js_round(value: f64) -> i64 {
    value.round() as i64
}

fn split_js_whitespace(text: &str) -> Vec<&str> {
    text.split(|character: char| pi_js_compat::string::is_js_whitespace_char(character))
        .filter(|part| !part.is_empty())
        .collect()
}

fn slice_utf16(text: &str, limit: usize) -> String {
    if utf16_len(text) <= limit {
        return text.to_owned();
    }
    let mut units = 0;
    let mut result = String::new();
    for character in text.chars() {
        let width = character.len_utf16();
        if units + width > limit {
            break;
        }
        units += width;
        result.push(character);
    }
    result
}

fn normalize(text: &str) -> String {
    js_trim(&text.replace(['\n', '\t'], " ")).to_owned()
}

fn extract_blocks_content(blocks: &[TextOrImageContent]) -> String {
    let mut result = String::new();
    for block in blocks {
        if let TextOrImageContent::Text(text) = block {
            result.push_str(&text.text);
        }
    }
    result
}

fn extract_value_content(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .map(|block| block.get("text").and_then(Value::as_str).unwrap_or_default().to_owned())
            .collect(),
        _ => String::new(),
    }
}

fn extract_message_full_content(message: &AgentMessage) -> String {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => extract_blocks_content(blocks),
        },
        AgentMessage::Assistant(assistant) => assistant
            .content
            .iter()
            .filter_map(|content| match content {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect(),
        AgentMessage::ToolResult(result) => extract_blocks_content(&result.content),
        AgentMessage::Custom(object) => object.get("content").map(extract_value_content).unwrap_or_default(),
    }
}

fn extract_assistant_content(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

fn extract_custom_message_content(content: &CustomMessageContent) -> String {
    match content {
        CustomMessageContent::Text(text) => text.clone(),
        CustomMessageContent::Blocks(blocks) => extract_blocks_content(blocks),
    }
}

fn shorten_path(path: &str) -> String {
    let home = pi_js_compat::process_env::get("HOME")
        .or_else(|| pi_js_compat::process_env::get("USERPROFILE"))
        .unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_owned()
}

fn format_tool_call(name: &str, args: &Value) -> String {
    let string_arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    let path = || {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .or_else(|| {
                args.get("file_path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
            })
            .unwrap_or_default();
        shorten_path(path)
    };
    match name {
        "read" => {
            let path = path();
            let offset = args.get("offset").and_then(Value::as_f64);
            let limit = args.get("limit").and_then(Value::as_f64);
            let mut display = path;
            if offset.is_some() || limit.is_some() {
                let start = offset.unwrap_or(1.0);
                let end = limit.map(|limit| start + limit - 1.0);
                display.push_str(&format!(
                    ":{}{}",
                    pi_js_compat::number::js_number_to_string(start),
                    end.map_or_else(String::new, |end| format!(
                        "-{}",
                        pi_js_compat::number::js_number_to_string(end)
                    ))
                ));
            }
            format!("[read: {display}]")
        }
        "write" => format!("[write: {}]", path()),
        "edit" => format!("[edit: {}]", path()),
        "bash" => {
            let raw_command = string_arg("command");
            let command = slice_utf16(&normalize_script(raw_command), 50);
            let suffix = if utf16_len(raw_command) > 50 { "..." } else { "" };
            format!("[bash: {command}{suffix}]")
        }
        "grep" => {
            let pattern = string_arg("pattern");
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .unwrap_or(".");
            format!("[grep: /{pattern}/ in {}]", shorten_path(path))
        }
        "find" => {
            let pattern = string_arg("pattern");
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .unwrap_or(".");
            format!("[find: {pattern} in {}]", shorten_path(path))
        }
        "ls" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .unwrap_or(".");
            format!("[ls: {}]", shorten_path(path))
        }
        _ => {
            let serialized = serde_json::to_string(args).unwrap_or_default();
            let short = slice_utf16(&serialized, 40);
            let suffix = if utf16_len(&serialized) > 40 { "..." } else { "" };
            format!("[{name}: {short}{suffix}]")
        }
    }
}

fn normalize_script(text: &str) -> String {
    js_trim(&text.replace(['\n', '\t'], " ")).to_owned()
}

fn format_label_timestamp(timestamp: &str, now_millis: i64) -> String {
    let now = local_date_fields(now_millis);
    let Some(date) = date_parse_millis(timestamp).and_then(local_date_fields) else {
        // Every getter of an invalid `Date` is NaN, and NaN never compares equal.
        return "aN/NaN/NaN NaN:NaN".to_owned();
    };
    let time = format!("{:02}:{:02}", date.hours, date.minutes);
    let same_year = now.is_some_and(|now| now.year == date.year);
    if same_year && now.is_some_and(|now| now.month == date.month && now.day == date.day) {
        return time;
    }
    let month = date.month + 1;
    let day = date.day;
    if same_year {
        return format!("{month}/{day} {time}");
    }
    let full_year = date.year.to_string();
    let year = &full_year[full_year.len().saturating_sub(2)..];
    format!("{year}/{month}/{day} {time}")
}

fn entry_message(entry: &SessionEntry) -> Option<&AgentMessage> {
    entry.as_message().map(|entry| entry.message.as_ref())
}

fn assistant_message(entry: &SessionEntry) -> Option<&AssistantMessage> {
    match entry_message(entry)? {
        AgentMessage::Assistant(message) => Some(message),
        _ => None,
    }
}

fn has_text_content(message: &AssistantMessage) -> bool {
    message.content.iter().any(|content| match content {
        AssistantContent::Text(text) => !js_trim(&text.text).is_empty(),
        _ => false,
    })
}

fn is_settings_entry(entry: &SessionEntry) -> bool {
    matches!(
        entry.entry_type(),
        "label" | "custom" | "model_change" | "thinking_level_change" | "session_info"
    )
}

/// Tree list component with selection and ASCII art visualization.
pub struct TreeList {
    flat_nodes: Vec<FlatNode>,
    filtered_indices: Vec<usize>,
    selected_index: usize,
    current_leaf_id: Option<String>,
    max_visible_lines: usize,
    filter_mode: FilterMode,
    search_query: String,
    tool_call_map: HashMap<String, ToolCallInfo>,
    multiple_roots: bool,
    show_label_timestamps: bool,
    active_path_ids: HashSet<String>,
    visible_parent_map: HashMap<String, Option<String>>,
    visible_children_map: HashMap<Option<String>, Vec<String>>,
    last_selected_id: Option<String>,
    folded_nodes: HashSet<String>,
    is_compacted_user_view: bool,
    pub on_select: Option<Rc<dyn Fn(String)>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_copy: Option<CopyHandler>,
    pub on_label_edit: Option<LabelEditHandler>,
}

impl TreeList {
    pub fn new(
        tree: Vec<SessionTreeNode>,
        current_leaf_id: Option<String>,
        max_visible_lines: usize,
        initial_selected_id: Option<String>,
        initial_filter_mode: Option<FilterMode>,
    ) -> Self {
        let multiple_roots = tree.len() > 1;
        let mut list = Self {
            flat_nodes: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            current_leaf_id,
            max_visible_lines,
            filter_mode: initial_filter_mode.unwrap_or(FilterMode::Default),
            search_query: String::new(),
            tool_call_map: HashMap::new(),
            multiple_roots,
            show_label_timestamps: false,
            active_path_ids: HashSet::new(),
            visible_parent_map: HashMap::new(),
            visible_children_map: HashMap::new(),
            last_selected_id: None,
            folded_nodes: HashSet::new(),
            is_compacted_user_view: false,
            on_select: None,
            on_cancel: None,
            on_copy: None,
            on_label_edit: None,
        };
        list.flat_nodes = list.flatten_tree(&tree);
        list.build_active_path();
        list.apply_filter();

        let target_id = initial_selected_id.or_else(|| list.current_leaf_id.clone());
        list.selected_index = list.find_nearest_visible_index(target_id.as_deref());
        list.last_selected_id = list
            .filtered_indices
            .get(list.selected_index)
            .map(|index| list.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned());
        list
    }

    fn find_nearest_visible_index(&self, entry_id: Option<&str>) -> usize {
        if self.filtered_indices.is_empty() {
            return 0;
        }
        let entry_map: HashMap<&str, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().unwrap_or_default(), index))
            .collect();
        let visible_id_to_index: HashMap<&str, usize> = self
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(index, flat_index)| (self.flat_nodes[*flat_index].node.entry.id().unwrap_or_default(), index))
            .collect();

        let mut current_id = entry_id;
        while let Some(id) = current_id {
            if let Some(index) = visible_id_to_index.get(id) {
                return *index;
            }
            let Some(node_index) = entry_map.get(id) else {
                break;
            };
            current_id = self.flat_nodes[*node_index].node.entry.parent_id();
        }

        self.filtered_indices.len() - 1
    }

    fn build_active_path(&mut self) {
        self.active_path_ids.clear();
        let Some(current_leaf_id) = &self.current_leaf_id else {
            return;
        };
        let entry_map: HashMap<String, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().unwrap_or_default().to_owned(), index))
            .collect();

        let mut current_id = Some(current_leaf_id.clone());
        while let Some(id) = current_id {
            self.active_path_ids.insert(id.clone());
            let Some(node_index) = entry_map.get(&id) else {
                break;
            };
            current_id = self.flat_nodes[*node_index].node.entry.parent_id().map(str::to_owned);
        }
    }

    fn flatten_tree(&mut self, roots: &[SessionTreeNode]) -> Vec<FlatNode> {
        let mut result = Vec::new();
        self.tool_call_map.clear();

        let leaf_id = self.current_leaf_id.clone();
        let mut contains_active: HashMap<String, bool> = HashMap::new();
        {
            let mut all_nodes: Vec<&SessionTreeNode> = Vec::new();
            let mut pre_order_stack: Vec<&SessionTreeNode> = roots.iter().collect();
            while let Some(node) = pre_order_stack.pop() {
                all_nodes.push(node);
                for child in node.children.iter().rev() {
                    pre_order_stack.push(child);
                }
            }
            for node in all_nodes.into_iter().rev() {
                let mut has = leaf_id.as_deref() == node.entry.id();
                for child in &node.children {
                    if contains_active
                        .get(child.entry.id().unwrap_or_default())
                        .copied()
                        .unwrap_or(false)
                    {
                        has = true;
                    }
                }
                contains_active.insert(node.entry.id().unwrap_or_default().to_owned(), has);
            }
        }

        let multiple_roots = roots.len() > 1;
        let mut ordered_roots: Vec<&SessionTreeNode> = roots.iter().collect();
        ordered_roots.sort_by_key(|node| {
            std::cmp::Reverse(
                contains_active
                    .get(node.entry.id().unwrap_or_default())
                    .copied()
                    .unwrap_or(false),
            )
        });

        type StackItem<'a> = (&'a SessionTreeNode, usize, bool, bool, bool, Vec<GutterInfo>, bool);
        let mut stack: Vec<StackItem> = Vec::new();
        for (index, root) in ordered_roots.iter().enumerate().rev() {
            let is_last = index == ordered_roots.len() - 1;
            stack.push((
                root,
                usize::from(multiple_roots),
                multiple_roots,
                multiple_roots,
                is_last,
                Vec::new(),
                multiple_roots,
            ));
        }

        while let Some((node, indent, just_branched, show_connector, is_last, gutters, is_virtual_root_child)) =
            stack.pop()
        {
            let entry = &node.entry;
            if let Some(message) = assistant_message(entry) {
                for content in &message.content {
                    if let AssistantContent::ToolCall(tool_call) = content {
                        self.tool_call_map.insert(
                            tool_call.id.clone(),
                            ToolCallInfo {
                                name: tool_call.name.clone(),
                                arguments: Value::Object(tool_call.arguments.clone()),
                            },
                        );
                    }
                }
            }

            result.push(FlatNode {
                node: node.clone(),
                indent,
                show_connector,
                is_last,
                gutters: gutters.clone(),
                is_virtual_root_child,
            });

            let children = &node.children;
            let multiple_children = children.len() > 1;
            let mut prioritized: Vec<&SessionTreeNode> = Vec::new();
            let mut rest: Vec<&SessionTreeNode> = Vec::new();
            for child in children {
                if contains_active
                    .get(child.entry.id().unwrap_or_default())
                    .copied()
                    .unwrap_or(false)
                {
                    prioritized.push(child);
                } else {
                    rest.push(child);
                }
            }
            let ordered_children: Vec<&SessionTreeNode> = prioritized.into_iter().chain(rest).collect();

            let child_indent = if multiple_children || (just_branched && indent > 0) {
                indent + 1
            } else {
                indent
            };

            let connector_displayed = show_connector && !is_virtual_root_child;
            let current_display_indent = if self.multiple_roots {
                indent.saturating_sub(1)
            } else {
                indent
            };
            let connector_position = current_display_indent.saturating_sub(1);
            let mut child_gutters = gutters;
            if connector_displayed {
                child_gutters.push(GutterInfo {
                    position: connector_position,
                    show: !is_last,
                });
            }

            for (index, child) in ordered_children.iter().enumerate().rev() {
                let child_is_last = index == ordered_children.len() - 1;
                stack.push((
                    child,
                    child_indent,
                    multiple_children,
                    multiple_children,
                    child_is_last,
                    child_gutters.clone(),
                    false,
                ));
            }
        }

        result
    }

    fn build_filter_candidates(&self) -> (Vec<usize>, bool) {
        if self.filter_mode != FilterMode::UserOnly {
            return ((0..self.flat_nodes.len()).collect(), false);
        }

        let entries: Vec<SessionEntry> = self.flat_nodes.iter().map(|flat| flat.node.entry.clone()).collect();
        let leaf = match self.current_leaf_id.as_deref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        let context_entries = build_context_entries(&entries, leaf);
        if !context_entries.iter().any(|entry| entry.entry_type() == "compaction") {
            return ((0..self.flat_nodes.len()).collect(), false);
        }

        let nodes_by_id: HashMap<String, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().unwrap_or_default().to_owned(), index))
            .collect();
        let nodes = context_entries
            .iter()
            .filter_map(|entry| nodes_by_id.get(entry.id().unwrap_or_default()).copied())
            .collect();
        (nodes, true)
    }

    fn should_hide_tool_only_assistant(&self, flat_node: &FlatNode) -> bool {
        let entry = &flat_node.node.entry;
        let Some(message) = assistant_message(entry) else {
            return false;
        };
        if Some(entry.id().unwrap_or_default()) == self.current_leaf_id.as_deref() {
            return false;
        }
        let has_text = has_text_content(message);
        let is_error_or_aborted = !matches!(message.stop_reason, StopReason::Stop | StopReason::ToolUse);
        !has_text && !is_error_or_aborted
    }

    fn matches_filter_mode(&self, flat_node: &FlatNode) -> bool {
        let entry = &flat_node.node.entry;
        let is_settings = is_settings_entry(entry);
        match self.filter_mode {
            FilterMode::UserOnly => {
                (entry.entry_type() == "message"
                    && entry_message(entry).is_some_and(|message| message.role() == "user"))
                    || (self.is_compacted_user_view && entry.entry_type() == "compaction")
            }
            FilterMode::NoTools => {
                !is_settings
                    && !(entry.entry_type() == "message"
                        && entry_message(entry).is_some_and(|m| m.role() == "toolResult"))
            }
            FilterMode::LabeledOnly => flat_node.node.label.is_some(),
            FilterMode::All => true,
            FilterMode::Default => !is_settings,
        }
    }

    fn matches_search(&self, flat_node: &FlatNode, search_tokens: &[String]) -> bool {
        if search_tokens.is_empty() {
            return true;
        }
        let node_text = self.get_searchable_text(&flat_node.node).to_lowercase();
        search_tokens.iter().all(|token| node_text.contains(token))
    }

    fn apply_filter(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.last_selected_id = self
                .filtered_indices
                .get(self.selected_index)
                .map(|index| self.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned())
                .or_else(|| self.last_selected_id.clone());
        }

        let search_tokens: Vec<String> = split_js_whitespace(&self.search_query.to_lowercase())
            .into_iter()
            .map(str::to_owned)
            .collect();
        let (candidates, is_compacted_user_view) = self.build_filter_candidates();
        self.is_compacted_user_view = is_compacted_user_view;

        let mut filtered: Vec<usize> = candidates
            .into_iter()
            .filter(|index| {
                let flat = &self.flat_nodes[*index];
                !self.should_hide_tool_only_assistant(flat)
                    && self.matches_filter_mode(flat)
                    && self.matches_search(flat, &search_tokens)
            })
            .collect();

        if !self.folded_nodes.is_empty() {
            let mut skip_set: HashSet<String> = HashSet::new();
            for flat in &self.flat_nodes {
                let id = flat.node.entry.id().unwrap_or_default().to_owned();
                let parent_id = flat.node.entry.parent_id();
                if let Some(parent_id) = parent_id
                    && (self.folded_nodes.contains(parent_id) || skip_set.contains(parent_id))
                {
                    skip_set.insert(id);
                }
            }
            filtered.retain(|index| !skip_set.contains(self.flat_nodes[*index].node.entry.id().unwrap_or_default()));
        }
        self.filtered_indices = filtered;

        self.recalculate_visual_structure();

        if let Some(last_selected_id) = self.last_selected_id.clone() {
            self.selected_index = self.find_nearest_visible_index(Some(&last_selected_id));
        } else if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len().saturating_sub(1);
        }

        if !self.filtered_indices.is_empty() {
            self.last_selected_id = self
                .filtered_indices
                .get(self.selected_index)
                .map(|index| self.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned())
                .or_else(|| self.last_selected_id.clone());
        }
    }

    fn recalculate_visual_structure(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        if self.is_compacted_user_view {
            self.recalculate_compacted_user_structure();
        } else {
            self.recalculate_legacy_visual_structure();
        }
    }

    fn recalculate_legacy_visual_structure(&mut self) {
        let visible_ids: HashSet<String> = self
            .filtered_indices
            .iter()
            .map(|index| self.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned())
            .collect();
        let entry_map: HashMap<String, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().unwrap_or_default().to_owned(), index))
            .collect();

        let find_visible_ancestor = |node_id: &str| -> Option<String> {
            let mut current_id = self
                .flat_nodes
                .get(*entry_map.get(node_id)?)
                .and_then(|flat| flat.node.entry.parent_id())
                .map(str::to_owned);
            while let Some(id) = current_id {
                if visible_ids.contains(&id) {
                    return Some(id);
                }
                current_id = entry_map
                    .get(&id)
                    .and_then(|index| self.flat_nodes[*index].node.entry.parent_id())
                    .map(str::to_owned);
            }
            None
        };

        let mut visible_parent: HashMap<String, Option<String>> = HashMap::new();
        let mut visible_children: HashMap<Option<String>, Vec<String>> = HashMap::new();
        visible_children.insert(None, Vec::new());

        for index in &self.filtered_indices {
            let node_id = self.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned();
            let ancestor_id = find_visible_ancestor(&node_id);
            visible_parent.insert(node_id.clone(), ancestor_id.clone());
            visible_children.entry(ancestor_id).or_default().push(node_id);
        }

        let visible_root_ids = visible_children.get(&None).cloned().unwrap_or_default();
        self.multiple_roots = visible_root_ids.len() > 1;

        type StackItem = (String, usize, bool, bool, bool, Vec<GutterInfo>, bool);
        let mut stack: Vec<StackItem> = Vec::new();
        for (index, root_id) in visible_root_ids.iter().enumerate().rev() {
            let is_last = index == visible_root_ids.len() - 1;
            stack.push((
                root_id.clone(),
                usize::from(self.multiple_roots),
                self.multiple_roots,
                self.multiple_roots,
                is_last,
                Vec::new(),
                self.multiple_roots,
            ));
        }

        while let Some((node_id, indent, just_branched, show_connector, is_last, gutters, is_virtual_root_child)) =
            stack.pop()
        {
            let Some(flat_index) = entry_map.get(&node_id).copied() else {
                continue;
            };
            {
                let flat = &mut self.flat_nodes[flat_index];
                flat.indent = indent;
                flat.show_connector = show_connector;
                flat.is_last = is_last;
                flat.gutters = gutters.clone();
                flat.is_virtual_root_child = is_virtual_root_child;
            }

            let children = visible_children
                .get(&Some(node_id.clone()))
                .cloned()
                .unwrap_or_default();
            let multiple_children = children.len() > 1;
            let child_indent = if multiple_children || (just_branched && indent > 0) {
                indent + 1
            } else {
                indent
            };

            let connector_displayed = show_connector && !is_virtual_root_child;
            let current_display_indent = if self.multiple_roots {
                indent.saturating_sub(1)
            } else {
                indent
            };
            let connector_position = current_display_indent.saturating_sub(1);
            let mut child_gutters = gutters;
            if connector_displayed {
                child_gutters.push(GutterInfo {
                    position: connector_position,
                    show: !is_last,
                });
            }

            for (index, child) in children.iter().enumerate().rev() {
                let child_is_last = index == children.len() - 1;
                stack.push((
                    child.clone(),
                    child_indent,
                    multiple_children,
                    multiple_children,
                    child_is_last,
                    child_gutters.clone(),
                    false,
                ));
            }
        }

        self.visible_parent_map = visible_parent;
        self.visible_children_map = visible_children;
    }

    fn recalculate_compacted_user_structure(&mut self) {
        let entry_ids: Vec<String> = self
            .filtered_indices
            .iter()
            .map(|index| self.flat_nodes[*index].node.entry.id().unwrap_or_default().to_owned())
            .collect();
        let mut visible_children: HashMap<Option<String>, Vec<String>> = HashMap::new();
        visible_children.insert(None, entry_ids.clone());

        self.multiple_roots = false;
        self.visible_parent_map = entry_ids.iter().map(|entry_id| (entry_id.clone(), None)).collect();
        for index in &self.filtered_indices {
            let flat = &mut self.flat_nodes[*index];
            flat.indent = 0;
            flat.show_connector = false;
            flat.is_last = true;
            flat.gutters = Vec::new();
            flat.is_virtual_root_child = false;
            visible_children.insert(Some(flat.node.entry.id().unwrap_or_default().to_owned()), Vec::new());
        }
        self.visible_children_map = visible_children;
    }

    fn get_searchable_text(&self, node: &SessionTreeNode) -> String {
        let entry = &node.entry;
        let mut parts: Vec<String> = Vec::new();
        if let Some(label) = &node.label {
            parts.push(label.clone());
        }

        match entry {
            SessionEntry::Known(KnownSessionEntry::Message(message)) => {
                let message = message.message.as_ref();
                parts.push(message.role().to_owned());
                parts.push(extract_message_full_content(message));
                if message.role() == "bashExecution"
                    && let AgentMessage::Custom(object) = message
                    && let Some(command) = object.get("command").and_then(Value::as_str)
                {
                    parts.push(command.to_owned());
                }
            }
            SessionEntry::Known(KnownSessionEntry::CustomMessage(entry)) => {
                parts.push(entry.custom_type.clone());
                parts.push(extract_custom_message_content(&entry.content));
            }
            SessionEntry::Known(KnownSessionEntry::Compaction(entry)) => {
                parts.push("compaction".to_owned());
                parts.push(entry.summary.clone());
            }
            SessionEntry::Known(KnownSessionEntry::BranchSummary(entry)) => {
                parts.push("branch summary".to_owned());
                parts.push(entry.summary.clone());
            }
            SessionEntry::Known(KnownSessionEntry::SessionInfo(entry)) => {
                parts.push("title".to_owned());
                if let Some(name) = &entry.name {
                    parts.push(name.clone());
                }
            }
            SessionEntry::Known(KnownSessionEntry::ModelChange(entry)) => {
                parts.push("model".to_owned());
                parts.push(entry.model_id.clone());
            }
            SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => {
                parts.push("thinking".to_owned());
                parts.push(entry.thinking_level.clone());
            }
            SessionEntry::Known(KnownSessionEntry::Custom(entry)) => {
                parts.push("custom".to_owned());
                parts.push(entry.custom_type.clone());
            }
            SessionEntry::Known(KnownSessionEntry::Label(entry)) => {
                parts.push("label".to_owned());
                parts.push(entry.label.clone().unwrap_or_default());
            }
            SessionEntry::Unknown(_) => {}
        }

        parts.join(" ")
    }

    pub fn get_search_query(&self) -> &str {
        &self.search_query
    }

    pub fn selected_entry_id(&self) -> Option<&str> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|index| self.flat_nodes[*index].node.entry.id())
    }

    pub fn get_selected_node(&self) -> Option<&SessionTreeNode> {
        self.filtered_indices
            .get(self.selected_index)
            .map(|index| &self.flat_nodes[*index].node)
    }

    pub fn copy_selected(&self) {
        let node = self.get_selected_node();
        let text = node.and_then(|node| self.get_entry_copy_text(node));
        if let Some(on_copy) = &self.on_copy {
            on_copy(text);
        }
    }

    pub fn update_node_label(&mut self, entry_id: &str, label: Option<&str>, label_timestamp: Option<&str>) {
        for flat in &mut self.flat_nodes {
            if flat.node.entry.id() == Some(entry_id) {
                flat.node.label = label.map(str::to_owned);
                flat.node.label_timestamp = label.map(|_| {
                    label_timestamp
                        .map(str::to_owned)
                        .unwrap_or_else(crate::utils::iso_time::now_iso_string)
                });
                break;
            }
        }
    }

    fn get_status_labels(&self) -> String {
        let mut labels = String::new();
        labels.push_str(self.filter_mode.label());
        if self.show_label_timestamps {
            labels.push_str(" [+label time]");
        }
        labels
    }

    fn get_entry_copy_text(&self, node: &SessionTreeNode) -> Option<String> {
        let entry = &node.entry;
        let text = match entry {
            SessionEntry::Known(KnownSessionEntry::Message(message)) => {
                let message = message.message.as_ref();
                if message.role() == "bashExecution" {
                    match message {
                        AgentMessage::Custom(object) => {
                            object.get("command").and_then(Value::as_str).map(str::to_owned)
                        }
                        _ => None,
                    }
                } else {
                    let text = extract_message_full_content(message);
                    if text.is_empty() {
                        assistant_message(entry).and_then(|message| message.error_message.clone())
                    } else {
                        Some(text)
                    }
                }
            }
            SessionEntry::Known(KnownSessionEntry::CustomMessage(entry)) => {
                Some(extract_custom_message_content(&entry.content))
            }
            SessionEntry::Known(KnownSessionEntry::Compaction(entry)) => Some(entry.summary.clone()),
            SessionEntry::Known(KnownSessionEntry::BranchSummary(entry)) => Some(entry.summary.clone()),
            _ => None,
        };
        text.filter(|text| !js_trim(text).is_empty())
    }

    fn get_entry_display_text(&self, node: &SessionTreeNode, is_selected: bool) -> String {
        let entry = &node.entry;
        let result = match entry {
            SessionEntry::Known(KnownSessionEntry::Message(message)) => {
                let message = message.message.as_ref();
                match message.role() {
                    "user" => {
                        with_theme(|theme| theme.fg(ThemeColor::Accent, "user: "))
                            + &normalize(&slice_utf16(&extract_message_full_content(message), 200))
                    }
                    "assistant" => {
                        let message = assistant_message(entry).expect("assistant entry");
                        let text_content = normalize(&slice_utf16(&extract_assistant_content(message), 200));
                        if !text_content.is_empty() {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: ")) + &text_content
                        } else if message.stop_reason == StopReason::Aborted {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Muted, "(aborted)"))
                        } else if let Some(error_message) = &message.error_message {
                            let error = slice_utf16(&normalize(error_message), 80);
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Error, &error))
                        } else {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Muted, "(no content)"))
                        }
                    }
                    "toolResult" => {
                        let tool_call = match message {
                            AgentMessage::ToolResult(result) => self.tool_call_map.get(&result.tool_call_id),
                            _ => None,
                        };
                        if let Some(tool_call) = tool_call {
                            with_theme(|theme| {
                                theme.fg(
                                    ThemeColor::Muted,
                                    &format_tool_call(&tool_call.name, &tool_call.arguments),
                                )
                            })
                        } else {
                            let tool_name = match message {
                                AgentMessage::ToolResult(result) => result.tool_name.clone(),
                                _ => "tool".to_owned(),
                            };
                            with_theme(|theme| theme.fg(ThemeColor::Muted, &format!("[{tool_name}]")))
                        }
                    }
                    "bashExecution" => {
                        let command = match message {
                            AgentMessage::Custom(object) => {
                                object.get("command").and_then(Value::as_str).unwrap_or_default()
                            }
                            _ => "",
                        };
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("[bash]: {}", normalize(command))))
                    }
                    role => with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("[{role}]"))),
                }
            }
            SessionEntry::Known(KnownSessionEntry::CustomMessage(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::CustomMessageLabel, &format!("[{}]: ", entry.custom_type)))
                    + &normalize(&extract_custom_message_content(&entry.content))
            }
            SessionEntry::Known(KnownSessionEntry::Compaction(entry)) => {
                let tokens = js_round(entry.tokens_before / 1000.0);
                with_theme(|theme| theme.fg(ThemeColor::BorderAccent, &format!("[compaction: {tokens}k tokens]: ")))
                    + &normalize(&entry.summary)
            }
            SessionEntry::Known(KnownSessionEntry::BranchSummary(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Warning, "[branch summary]: ")) + &normalize(&entry.summary)
            }
            SessionEntry::Known(KnownSessionEntry::ModelChange(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("[model: {}]", entry.model_id)))
            }
            SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("[thinking: {}]", entry.thinking_level)))
            }
            SessionEntry::Known(KnownSessionEntry::Custom(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("[custom: {}]", entry.custom_type)))
            }
            SessionEntry::Known(KnownSessionEntry::Label(entry)) => with_theme(|theme| {
                theme.fg(
                    ThemeColor::Dim,
                    &format!("[label: {}]", entry.label.as_deref().unwrap_or("(cleared)")),
                )
            }),
            SessionEntry::Known(KnownSessionEntry::SessionInfo(entry)) => match &entry.name {
                Some(name) => with_theme(|theme| {
                    theme.fg(ThemeColor::Dim, "[title: ")
                        + &theme.fg(ThemeColor::Dim, name)
                        + &theme.fg(ThemeColor::Dim, "]")
                }),
                None => with_theme(|theme| {
                    theme.fg(ThemeColor::Dim, "[title: ")
                        + &theme.italic(&theme.fg(ThemeColor::Dim, "empty"))
                        + &theme.fg(ThemeColor::Dim, "]")
                }),
            },
            _ => String::new(),
        };

        if is_selected {
            with_theme(|theme| theme.bold(&result))
        } else {
            result
        }
    }

    fn is_foldable(&self, entry_id: &str) -> bool {
        let Some(children) = self.visible_children_map.get(&Some(entry_id.to_owned())) else {
            return false;
        };
        if children.is_empty() {
            return false;
        }
        match self.visible_parent_map.get(entry_id) {
            Some(Some(parent_id)) => self
                .visible_children_map
                .get(&Some(parent_id.clone()))
                .is_some_and(|siblings| siblings.len() > 1),
            _ => true,
        }
    }

    fn find_branch_segment_start(&self, direction: BranchDirection) -> usize {
        let Some(selected_id) = self.selected_entry_id().map(str::to_owned) else {
            return self.selected_index;
        };
        let index_by_entry_id: HashMap<String, usize> = self
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(index, flat_index)| {
                (
                    self.flat_nodes[*flat_index]
                        .node
                        .entry
                        .id()
                        .unwrap_or_default()
                        .to_owned(),
                    index,
                )
            })
            .collect();
        let mut current_id = selected_id;
        if direction == BranchDirection::Down {
            loop {
                let children: Vec<String> = self
                    .visible_children_map
                    .get(&Some(current_id.clone()))
                    .cloned()
                    .unwrap_or_default();
                if children.is_empty() {
                    return index_by_entry_id
                        .get(&current_id)
                        .copied()
                        .unwrap_or(self.selected_index);
                }
                if children.len() > 1 {
                    return index_by_entry_id
                        .get(&children[0])
                        .copied()
                        .unwrap_or(self.selected_index);
                }
                current_id = children[0].clone();
            }
        }

        loop {
            let parent_id = self.visible_parent_map.get(&current_id).cloned().flatten();
            let Some(parent_id) = parent_id else {
                return index_by_entry_id
                    .get(&current_id)
                    .copied()
                    .unwrap_or(self.selected_index);
            };
            let children = self
                .visible_children_map
                .get(&Some(parent_id.clone()))
                .cloned()
                .unwrap_or_default();
            if children.len() > 1 {
                let segment_start = index_by_entry_id
                    .get(&current_id)
                    .copied()
                    .unwrap_or(self.selected_index);
                if segment_start < self.selected_index {
                    return segment_start;
                }
            }
            current_id = parent_id;
        }
    }

    pub fn handle_input(&mut self, key_data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") {
            self.selected_index = if self.selected_index == 0 {
                self.filtered_indices.len().saturating_sub(1)
            } else {
                self.selected_index - 1
            };
        } else if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = if self.selected_index == self.filtered_indices.len().saturating_sub(1) {
                0
            } else {
                self.selected_index + 1
            };
        } else if keybindings.matches(key_data, "app.tree.foldOrUp") {
            let current_id = self.selected_entry_id().map(str::to_owned);
            if let Some(current_id) = current_id
                && self.is_foldable(&current_id)
                && !self.folded_nodes.contains(&current_id)
            {
                self.folded_nodes.insert(current_id);
                self.apply_filter();
            } else {
                self.selected_index = self.find_branch_segment_start(BranchDirection::Up);
            }
        } else if keybindings.matches(key_data, "app.tree.unfoldOrDown") {
            let current_id = self.selected_entry_id().map(str::to_owned);
            if let Some(current_id) = current_id
                && self.folded_nodes.contains(&current_id)
            {
                self.folded_nodes.remove(&current_id);
                self.apply_filter();
            } else {
                self.selected_index = self.find_branch_segment_start(BranchDirection::Down);
            }
        } else if keybindings.matches(key_data, "tui.editor.cursorLeft")
            || keybindings.matches(key_data, "tui.select.pageUp")
        {
            self.selected_index = self.selected_index.saturating_sub(self.max_visible_lines);
        } else if keybindings.matches(key_data, "tui.editor.cursorRight")
            || keybindings.matches(key_data, "tui.select.pageDown")
        {
            self.selected_index =
                (self.selected_index + self.max_visible_lines).min(self.filtered_indices.len().saturating_sub(1));
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(entry_id) = self.selected_entry_id().map(str::to_owned)
                && let Some(on_select) = &self.on_select
            {
                on_select(entry_id);
            }
        } else if keybindings.matches(key_data, "app.message.copy") {
            self.copy_selected();
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            if !self.search_query.is_empty() {
                self.search_query.clear();
                self.folded_nodes.clear();
                self.apply_filter();
            } else if let Some(on_cancel) = &self.on_cancel {
                on_cancel();
            }
        } else if keybindings.matches(key_data, "app.tree.filter.default") {
            self.filter_mode = FilterMode::Default;
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.noTools") {
            self.filter_mode = if self.filter_mode == FilterMode::NoTools {
                FilterMode::Default
            } else {
                FilterMode::NoTools
            };
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.userOnly") {
            self.filter_mode = if self.filter_mode == FilterMode::UserOnly {
                FilterMode::Default
            } else {
                FilterMode::UserOnly
            };
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.labeledOnly") {
            self.filter_mode = if self.filter_mode == FilterMode::LabeledOnly {
                FilterMode::Default
            } else {
                FilterMode::LabeledOnly
            };
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.all") {
            self.filter_mode = if self.filter_mode == FilterMode::All {
                FilterMode::Default
            } else {
                FilterMode::All
            };
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.cycleBackward") {
            let modes = FilterMode::ALL_MODES;
            let current = modes.iter().position(|mode| *mode == self.filter_mode).unwrap_or(0);
            self.filter_mode = modes[(current + modes.len() - 1) % modes.len()];
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "app.tree.filter.cycleForward") {
            let modes = FilterMode::ALL_MODES;
            let current = modes.iter().position(|mode| *mode == self.filter_mode).unwrap_or(0);
            self.filter_mode = modes[(current + 1) % modes.len()];
            self.folded_nodes.clear();
            self.apply_filter();
        } else if keybindings.matches(key_data, "tui.editor.deleteCharBackward") {
            if !self.search_query.is_empty() {
                self.search_query.pop();
                self.folded_nodes.clear();
                self.apply_filter();
            }
        } else if keybindings.matches(key_data, "app.tree.editLabel") {
            if let Some(node) = self.get_selected_node()
                && let Some(on_label_edit) = &self.on_label_edit
            {
                on_label_edit(node.entry.id().unwrap_or_default(), node.label.clone().as_deref());
            }
        } else if keybindings.matches(key_data, "app.tree.toggleLabelTimestamp") {
            self.show_label_timestamps = !self.show_label_timestamps;
        } else {
            let has_control_chars = key_data.chars().any(|character| {
                let code = character as u32;
                code < 32 || code == 0x7f || (0x80..=0x9f).contains(&code)
            });
            if !has_control_chars && !key_data.is_empty() {
                self.search_query.push_str(key_data);
                self.folded_nodes.clear();
                self.apply_filter();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchDirection {
    Up,
    Down,
}

impl Component for TreeList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        if self.filtered_indices.is_empty() {
            lines.push(truncate(
                &with_theme(|theme| theme.fg(ThemeColor::Muted, "  No entries found")),
                width,
                "",
            ));
            lines.push(truncate(
                &with_theme(|theme| theme.fg(ThemeColor::Muted, &format!("  (0/0){}", self.get_status_labels()))),
                width,
                "",
            ));
            return lines;
        }

        let start_index = self
            .selected_index
            .saturating_sub(self.max_visible_lines / 2)
            .min(self.filtered_indices.len().saturating_sub(self.max_visible_lines));
        let end_index = (start_index + self.max_visible_lines).min(self.filtered_indices.len());

        let mut rendered_rows = Vec::new();
        for i in start_index..end_index {
            let flat_index = self.filtered_indices[i];
            let flat_node = &self.flat_nodes[flat_index];
            let entry = &flat_node.node.entry;
            let is_selected = i == self.selected_index;
            let entry_id = entry.id().unwrap_or_default();

            let cursor = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
            } else {
                "  ".to_owned()
            };

            let display_indent = if self.multiple_roots {
                flat_node.indent.saturating_sub(1)
            } else {
                flat_node.indent
            };

            let connector = if flat_node.show_connector && !flat_node.is_virtual_root_child {
                if flat_node.is_last { "└─ " } else { "├─ " }
            } else {
                ""
            };
            let connector_position = if connector.is_empty() {
                None
            } else {
                Some(display_indent.saturating_sub(1))
            };

            let total_chars = display_indent * 3;
            let is_folded = self.folded_nodes.contains(entry_id);
            let mut prefix_chars = String::new();
            for character_index in 0..total_chars {
                let level = character_index / 3;
                let position_in_level = character_index % 3;
                let gutter = flat_node.gutters.iter().find(|gutter| gutter.position == level);
                if let Some(gutter) = gutter {
                    if position_in_level == 0 {
                        prefix_chars.push(if gutter.show { '│' } else { ' ' });
                    } else {
                        prefix_chars.push(' ');
                    }
                } else if connector_position == Some(level) {
                    if position_in_level == 0 {
                        prefix_chars.push(if flat_node.is_last { '└' } else { '├' });
                    } else if position_in_level == 1 {
                        let foldable = self.is_foldable(entry_id);
                        prefix_chars.push(if is_folded {
                            '⊞'
                        } else if foldable {
                            '⊟'
                        } else {
                            '─'
                        });
                    } else {
                        prefix_chars.push(' ');
                    }
                } else {
                    prefix_chars.push(' ');
                }
            }

            let shows_fold_in_connector = flat_node.show_connector && !flat_node.is_virtual_root_child;
            let fold_marker = if is_folded && !shows_fold_in_connector {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "⊞ "))
            } else {
                String::new()
            };
            let path_marker = if self.active_path_ids.contains(entry_id) {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "• "))
            } else {
                String::new()
            };
            let label = flat_node
                .node
                .label
                .as_deref()
                .map(|label| with_theme(|theme| theme.fg(ThemeColor::Warning, &format!("[{label}] "))))
                .unwrap_or_default();
            let label_timestamp = if self.show_label_timestamps {
                flat_node
                    .node
                    .label
                    .as_deref()
                    .zip(flat_node.node.label_timestamp.as_deref())
                    .map(|(_, timestamp)| {
                        with_theme(|theme| {
                            theme.fg(
                                ThemeColor::Muted,
                                &format!("{} ", format_label_timestamp(timestamp, pi_ai::request::now_millis())),
                            )
                        })
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let content = self.get_entry_display_text(&flat_node.node, is_selected);
            let prefix_part =
                with_theme(|theme| theme.fg(ThemeColor::Dim, &prefix_chars)) + &fold_marker + &path_marker;
            let anchor_col = visible_width(&prefix_part);
            let mut gutter = cursor;
            let mut body = prefix_part + &label + &label_timestamp + &content;
            if is_selected {
                gutter = with_theme(|theme| theme.bg(ThemeBg::SelectedBg, &gutter));
                body = with_theme(|theme| theme.bg(ThemeBg::SelectedBg, &body));
            }
            let body_width = visible_width(&body);
            rendered_rows.push(HorizontalViewportRow {
                gutter,
                body,
                anchor_col,
                body_width,
                is_selected,
            });
        }

        lines.extend(render_horizontal_viewport(&rendered_rows, width));
        lines.push(truncate(
            &with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    &format!(
                        "  ({}/{}){}",
                        self.selected_index + 1,
                        self.filtered_indices.len(),
                        self.get_status_labels()
                    ),
                )
            }),
            width,
            "",
        ));
        lines
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn invalidate(&mut self) {}
}

/// Component that displays the current search query.
pub struct SearchLine {
    tree_list: Rc<RefCell<TreeList>>,
}

impl SearchLine {
    pub fn new(tree_list: Rc<RefCell<TreeList>>) -> Self {
        Self { tree_list }
    }
}

impl Component for SearchLine {
    fn render(&mut self, width: usize) -> Vec<String> {
        let query = self.tree_list.borrow().get_search_query().to_owned();
        if query.is_empty() {
            return vec![truncate(
                &format!(
                    "  {}",
                    with_theme(|theme| theme.fg(ThemeColor::Muted, "Type to search:"))
                ),
                width,
                "",
            )];
        }
        vec![truncate(
            &format!(
                "  {} {}",
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Type to search:")),
                with_theme(|theme| theme.fg(ThemeColor::Accent, &query))
            ),
            width,
            "",
        )]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}

const TREE_HELP_ITEMS: [(&[&str], &str, bool); 8] = [
    (&["tui.select.up", "tui.select.down"], "move", false),
    (&["tui.editor.cursorLeft", "tui.editor.cursorRight"], "page", false),
    (&["app.tree.foldOrUp", "app.tree.unfoldOrDown"], "branch", false),
    (&["app.message.copy"], "copy", false),
    (&["app.tree.editLabel"], "label", false),
    (&["app.tree.toggleLabelTimestamp"], "label time", false),
    (
        &[
            "app.tree.filter.default",
            "app.tree.filter.noTools",
            "app.tree.filter.userOnly",
            "app.tree.filter.labeledOnly",
            "app.tree.filter.all",
        ],
        "filters",
        true,
    ),
    (
        &["app.tree.filter.cycleForward", "app.tree.filter.cycleBackward"],
        "cycle",
        true,
    ),
];

fn help_replacement_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\bpageUp\b|\bpageDown\b|\bup\b|\bdown\b|\bleft\b|\bright\b").expect("regex"))
}

fn format_help_keys(keybindings: &[&str]) -> String {
    let manager = get_keybindings();
    let keys: Vec<String> = keybindings
        .iter()
        .filter_map(|binding| manager.get_keys(binding).first().map(KeyId::to_string))
        .collect();
    if keys.is_empty() {
        return String::new();
    }

    let text = format_key_text(&compact_raw_keys(&keys), Default::default());
    help_replacement_pattern()
        .replace_all(&text, |captures: &regex::Captures| {
            let replacement: &str = match &captures[0] {
                "pageUp" => "pgup",
                "pageDown" => "pgdn",
                "up" => "↑",
                "down" => "↓",
                "left" => "←",
                "right" => "→",
                other => other,
            };
            replacement.to_owned()
        })
        .into_owned()
}

fn compact_raw_keys(keys: &[String]) -> String {
    if keys.len() == 1 {
        return keys[0].clone();
    }
    let parts: Vec<(String, String)> = keys
        .iter()
        .map(|key| match key.rfind('+') {
            Some(index) => (key[..=index].to_owned(), key[index + 1..].to_owned()),
            None => (String::new(), key.clone()),
        })
        .collect();
    let prefix = &parts[0].0;
    if !prefix.is_empty() && parts.iter().all(|part| &part.0 == prefix) {
        return format!(
            "{prefix}{}",
            parts.iter().map(|part| part.1.as_str()).collect::<Vec<_>>().join("/")
        );
    }
    keys.join("/")
}

/// Component that renders tree help as semantic rows with chunk-aware wrapping.
pub struct TreeHelp;

impl Component for TreeHelp {
    fn render(&mut self, width: usize) -> Vec<String> {
        let items: Vec<String> = TREE_HELP_ITEMS
            .iter()
            .map(|(keys, label, label_first)| {
                let text = format_help_keys(keys);
                if text.is_empty() {
                    (*label).to_owned()
                } else if *label_first {
                    format!("{label} {text}")
                } else {
                    format!("{text} {label}")
                }
            })
            .collect();

        let available_width = width.max(1);
        let indent = "  ";
        let separator = " · ";
        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();

        for item in items {
            let candidate = if current_line.is_empty() {
                let indented = format!("{indent}{item}");
                if visible_width(&indented) <= available_width {
                    indented
                } else {
                    item.clone()
                }
            } else {
                format!("{current_line}{separator}{item}")
            };
            if current_line.is_empty() || visible_width(&candidate) <= available_width {
                current_line = candidate;
                continue;
            }

            lines.extend(wrap_text_with_ansi(
                pi_js_compat::string::js_trim_end(&current_line),
                available_width,
            ));
            let indented = format!("{indent}{item}");
            current_line = if visible_width(&indented) <= available_width {
                indented
            } else {
                item
            };
        }

        if !current_line.is_empty() {
            lines.extend(wrap_text_with_ansi(
                pi_js_compat::string::js_trim_end(&current_line),
                available_width,
            ));
        }

        lines
            .into_iter()
            .map(|line| with_theme(|theme| theme.fg(ThemeColor::Muted, &line)))
            .collect()
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}

/// Label input component shown when editing a label.
pub struct LabelInput {
    input: Input,
    entry_id: String,
    pub on_submit: Option<LabelEditHandler>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    focused: bool,
}

impl LabelInput {
    pub fn new(entry_id: &str, current_label: Option<&str>) -> Self {
        let mut input = Input::new(InputOptions::default());
        if let Some(label) = current_label {
            input.set_value(label);
        }
        Self {
            input,
            entry_id: entry_id.to_owned(),
            on_submit: None,
            on_cancel: None,
            focused: false,
        }
    }

    pub fn input(&self) -> &Input {
        &self.input
    }
}

impl Component for LabelInput {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let indent = "  ";
        let available_width = width.saturating_sub(indent.len());
        lines.push(truncate(
            &format!(
                "{indent}{}",
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Label (empty to remove):"))
            ),
            width,
            "",
        ));
        for line in self.input.render(available_width) {
            lines.push(truncate(&format!("{indent}{line}"), width, ""));
        }
        lines.push(truncate(
            &format!(
                "{indent}{}  {}",
                key_hint("tui.select.confirm", "save"),
                key_hint("tui.select.cancel", "cancel")
            ),
            width,
            "",
        ));
        lines
    }

    fn handle_input(&mut self, key_data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.confirm") {
            let value = js_trim(self.input.get_value()).to_owned();
            let label = (!value.is_empty()).then_some(value);
            if let Some(on_submit) = &self.on_submit {
                on_submit(&self.entry_id, label.as_deref());
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            if let Some(on_cancel) = &self.on_cancel {
                on_cancel();
            }
        } else {
            self.input.handle_input(key_data);
        }
    }

    fn invalidate(&mut self) {}

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Focusable for LabelInput {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.input.set_focused(focused);
    }
}

/// Component that renders a session tree selector for navigation.
pub struct TreeSelectorComponent {
    tree_list: Rc<RefCell<TreeList>>,
    label_input: Rc<RefCell<Option<ComponentRef>>>,
    label_input_container: ComponentRef,
    tree_container: ComponentRef,
    on_label_change: Option<LabelEditHandler>,
    pub on_copy: Option<CopyHandler>,
    focused: bool,
    container: Container,
    empty_cancel_at: Option<Instant>,
}

impl TreeSelectorComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tree: Vec<SessionTreeNode>,
        current_leaf_id: Option<String>,
        terminal_height: usize,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        on_label_change: Option<LabelEditHandler>,
        initial_selected_id: Option<String>,
        initial_filter_mode: Option<FilterMode>,
    ) -> Self {
        let max_visible_lines = 5.max(terminal_height / 2);
        let mut tree_list = TreeList::new(
            tree.clone(),
            current_leaf_id,
            max_visible_lines,
            initial_selected_id,
            initial_filter_mode,
        );
        tree_list.on_select = Some(on_select);
        tree_list.on_cancel = Some(on_cancel);
        let tree_list = Rc::new(RefCell::new(tree_list));

        let mut component = Self {
            tree_list,
            label_input: Rc::new(RefCell::new(None)),
            label_input_container: component(Container::new()),
            tree_container: component(Container::new()),
            on_label_change,
            on_copy: None,
            focused: false,
            container: Container::new(),
            empty_cancel_at: None,
        };
        component.wire_label_edit();
        component.rebuild_layout();
        if tree.is_empty() {
            component.empty_cancel_at = Some(Instant::now() + Duration::from_millis(100));
        }
        component
    }

    fn wire_label_edit(&mut self) {
        let tree_list = Rc::clone(&self.tree_list);
        let label_input = Rc::clone(&self.label_input);
        let label_input_container = Rc::clone(&self.label_input_container);
        let tree_container = Rc::clone(&self.tree_container);
        let on_label_change = self.on_label_change.clone();
        let show = Rc::new(move |entry_id: &str, current_label: Option<&str>| {
            let mut input = LabelInput::new(entry_id, current_label);

            let submit_tree_list = Rc::clone(&tree_list);
            let submit_label_input = Rc::clone(&label_input);
            let submit_label_container = Rc::clone(&label_input_container);
            let submit_tree_container = Rc::clone(&tree_container);
            let submit_label_change = on_label_change.clone();
            input.on_submit = Some(Rc::new(move |id, label| {
                submit_tree_list.borrow_mut().update_node_label(id, label, None);
                if let Some(on_label_change) = &submit_label_change {
                    on_label_change(id, label);
                }
                *submit_label_input.borrow_mut() = None;
                if let Some(container) = submit_label_container
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<Container>())
                {
                    container.clear();
                }
                if let Some(container) = submit_tree_container
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<Container>())
                {
                    container.clear();
                    container.add_child(Rc::clone(&submit_tree_list) as ComponentRef);
                }
            }));

            let cancel_tree_list = Rc::clone(&tree_list);
            let cancel_label_input = Rc::clone(&label_input);
            let cancel_label_container = Rc::clone(&label_input_container);
            let cancel_tree_container = Rc::clone(&tree_container);
            input.on_cancel = Some(Rc::new(move || {
                *cancel_label_input.borrow_mut() = None;
                if let Some(container) = cancel_label_container
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<Container>())
                {
                    container.clear();
                }
                if let Some(container) = cancel_tree_container
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<Container>())
                {
                    container.clear();
                    container.add_child(Rc::clone(&cancel_tree_list) as ComponentRef);
                }
            }));

            let input = component(input);
            *label_input.borrow_mut() = Some(Rc::clone(&input));
            if let Some(container) = label_input_container
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Container>())
            {
                container.clear();
                container.add_child(input);
            }
        });
        self.tree_list.borrow_mut().on_label_edit = Some(show);
    }

    fn rebuild_layout(&mut self) {
        self.container.clear();
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(dynamic_border()));
        self.container.add_child(component(Text::new(
            with_theme(|theme| theme.bold("  Session Tree")),
            1,
            0,
        )));
        self.container.add_child(component(TreeHelp));
        self.container
            .add_child(component(SearchLine::new(Rc::clone(&self.tree_list))));
        self.container.add_child(component(dynamic_border()));
        self.container.add_child(component(Spacer::new(1)));

        if let Some(container) = self
            .tree_container
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Container>())
        {
            container.clear();
            container.add_child(Rc::clone(&self.tree_list) as ComponentRef);
        }
        self.container.add_child(Rc::clone(&self.tree_container));
        self.container.add_child(Rc::clone(&self.label_input_container));
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(dynamic_border()));
    }

    pub fn tree_list(&self) -> &Rc<RefCell<TreeList>> {
        &self.tree_list
    }

    pub fn set_on_copy(&mut self, on_copy: Option<CopyHandler>) {
        self.on_copy = on_copy.clone();
        let tree_list = Rc::clone(&self.tree_list);
        self.tree_list.borrow_mut().on_copy = on_copy.map(|on_copy| {
            let tree_list = Rc::clone(&tree_list);
            let _ = tree_list;
            Rc::new(move |text: Option<String>| on_copy(text)) as CopyHandler
        });
    }

    /// Fire the deferred empty-tree cancel once the 100ms delay elapsed.
    pub fn poll(&mut self, now: Instant) -> bool {
        if let Some(deadline) = self.empty_cancel_at
            && now >= deadline
        {
            self.empty_cancel_at = None;
            let cancel = self.tree_list.borrow().on_cancel.clone();
            if let Some(cancel) = cancel {
                cancel();
            }
            return true;
        }
        false
    }
}

impl Component for TreeSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn handle_input(&mut self, key_data: &str) {
        let label_input = self.label_input.borrow().clone();
        if let Some(label_input) = label_input {
            label_input.borrow_mut().handle_input(key_data);
        } else {
            self.tree_list.borrow_mut().handle_input(key_data);
        }
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}

impl Focusable for TreeSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        let label_input = self.label_input.borrow().clone();
        if let Some(label_input) = label_input
            && let Some(label_input) = label_input
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<LabelInput>())
        {
            label_input.set_focused(focused);
        }
    }
}
