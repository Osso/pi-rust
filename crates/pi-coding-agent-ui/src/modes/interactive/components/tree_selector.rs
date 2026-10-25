//! Ported from `coding-agent/src/modes/interactive/components/tree-selector.ts`.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextOrImageContent, UserContent};
use pi_js_compat::date::{date_parse_utf16, local_date_fields};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::js_regex::JsRegex;
use pi_js_compat::number::{js_math_max, js_math_min};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, Container, Focusable, component};
use pi_tui::utils::{slice_by_column, truncate_to_width_with, visible_width, wrap_text_with_ansi};

use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{format_key_text, key_hint};
use crate::core::messages::CustomMessageContent;
use crate::core::session_manager::{
    KnownSessionEntry, LeafSelection, SessionEntry, SessionTreeNode, build_context_entries,
};
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, with_theme};

/// Called with `(entryId, label)` when a label is edited.
pub type LabelEditHandler = Rc<dyn Fn(&JsString, Option<&JsString>)>;

const TREE_GUTTER_WIDTH: usize = 2;
const MIN_VISIBLE_ANCHOR_CONTENT_WIDTH: usize = 4;
const MAX_VISIBLE_ANCHOR_CONTENT_WIDTH: usize = 20;
const MIN_ANCHOR_CONTEXT_WIDTH: usize = 2;
const MAX_ANCHOR_CONTEXT_WIDTH: usize = 12;

fn truncate(text: &JsString, width: usize, ellipsis: &str) -> JsString {
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
    arguments: JsValue,
}

struct HorizontalViewportRow {
    gutter: JsString,
    body: JsString,
    anchor_col: usize,
    body_width: usize,
    is_selected: bool,
}

fn render_horizontal_viewport(rows: &[HorizontalViewportRow], width: usize) -> Vec<JsString> {
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
                js_format!(
                    "{}{}\u{1b}[0m",
                    row.gutter,
                    slice_by_column(&row.body, horizontal_scroll, viewport_width, true)
                )
            } else {
                js_format!("{}{}", row.gutter, row.body)
            };
            truncate(&line, width, "")
        })
        .collect()
}

fn js_round(value: f64) -> f64 {
    let lower = value.floor();
    let rounded = if value - lower < 0.5 { lower } else { lower + 1.0 };
    if rounded == 0.0 {
        0.0_f64.copysign(value)
    } else {
        rounded
    }
}

fn split_js_whitespace(text: &JsString) -> Vec<JsString> {
    text.units()
        .split(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
        .filter(|part| !part.is_empty())
        .map(|part| JsString::from_utf16(part.to_vec()))
        .collect()
}

fn slice_utf16(text: &JsString, limit: usize) -> JsString {
    text.slice(0, limit.min(text.len()))
}

fn normalize(text: &JsString) -> JsString {
    text.replace("\n", " ").replace("\t", " ").trim()
}

fn extract_blocks_content(blocks: &[TextOrImageContent]) -> JsString {
    let mut result = JsString::default();
    for block in blocks {
        if let TextOrImageContent::Text(text) = block {
            result.push_str(&text.text);
        }
    }
    result
}

fn extract_value_content(value: &JsValue) -> JsString {
    match value {
        JsValue::String(text) => text.clone(),
        JsValue::Array(blocks) => JsString::join(
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(JsValue::as_string) == Some(&"text".into()))
                .map(|block| {
                    block
                        .get("text")
                        .and_then(JsValue::as_string)
                        .cloned()
                        .unwrap_or_default()
                }),
            "",
        ),
        _ => JsString::default(),
    }
}

fn extract_message_full_content(message: &AgentMessage) -> JsString {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => extract_blocks_content(blocks),
        },
        AgentMessage::Assistant(assistant) => JsString::join(
            assistant.content.iter().filter_map(|content| match content {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            }),
            "",
        ),
        AgentMessage::ToolResult(result) => extract_blocks_content(&result.content),
        AgentMessage::Custom(object) => object.get("content").map(extract_value_content).unwrap_or_default(),
    }
}

fn extract_assistant_content(message: &AssistantMessage) -> JsString {
    JsString::join(
        message.content.iter().filter_map(|content| match content {
            AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }),
        "",
    )
}

fn extract_custom_message_content(content: &CustomMessageContent) -> JsString {
    match content {
        CustomMessageContent::Text(text) => text.clone(),
        CustomMessageContent::Blocks(blocks) => extract_blocks_content(blocks),
    }
}

fn shorten_path(path: &JsString) -> JsString {
    let home = pi_js_compat::process_env::get("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| pi_js_compat::process_env::get("USERPROFILE"))
        .unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        return JsString::from("~") + path.slice(home.encode_utf16().count(), path.len());
    }
    path.clone()
}

#[cfg(test)]
mod js_value_tests {
    use super::*;

    #[test]
    fn unknown_tool_label_stringifies_before_truncating() {
        let args = serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap();
        assert_eq!(format_tool_call("unknown", &args), r#"[unknown: {"1":1,"2":2}]"#);
    }
}

fn stringify_argument(value: &JsValue) -> JsString {
    js_json::to_js_string(value).unwrap_or_else(|message| crate::utils::uncaught::throw_uncaught("TypeError", message))
}

fn stringify_first_truthy(args: &JsValue, keys: &[&str], default: &str) -> JsString {
    keys.iter()
        .find_map(|key| args.get(key).filter(|value| js_json::is_truthy(value)))
        .map_or_else(|| default.into(), stringify_argument)
}

fn argument_primitive(value: &JsValue) -> JsValue {
    match value {
        JsValue::Array(_) | JsValue::Object(_) => JsValue::String(stringify_argument(value)),
        primitive => primitive.clone(),
    }
}

fn argument_number(value: &JsValue) -> f64 {
    js_json::to_js_number(value).unwrap_or_else(|message| crate::utils::uncaught::throw_uncaught("TypeError", message))
}

fn read_range_end(start: &JsValue, limit: &JsValue) -> f64 {
    let start = argument_primitive(start);
    let limit = argument_primitive(limit);
    let sum = if matches!(start, JsValue::String(_)) || matches!(limit, JsValue::String(_)) {
        argument_number(&JsValue::String(
            stringify_argument(&start) + stringify_argument(&limit),
        ))
    } else {
        argument_number(&start) + argument_number(&limit)
    };
    sum - 1.0
}

fn format_read_path(args: &JsValue, mut path: JsString) -> JsString {
    let offset = args.get("offset");
    let limit = args.get("limit");
    if offset.is_none() && limit.is_none() {
        return path;
    }
    let start = offset
        .filter(|value| !matches!(value, JsValue::Null))
        .cloned()
        .unwrap_or(JsValue::Number(1.0));
    let end = limit.map(|limit| read_range_end(&start, limit));
    path.push_unit(u16::from(b':'));
    path.push_str(stringify_argument(&start));
    if let Some(end) = end.filter(|end| *end != 0.0 && !end.is_nan()) {
        path.push_str(js_format!("-{}", pi_js_compat::number::js_number_to_string(end)));
    }
    path
}

fn format_tool_call(name: &str, args: &JsValue) -> JsString {
    match name {
        "read" | "write" | "edit" => {
            let path = shorten_path(&stringify_first_truthy(args, &["path", "file_path"], ""));
            let path = if name == "read" {
                format_read_path(args, path)
            } else {
                path
            };
            js_format!("[{}: {}]", name, path)
        }
        "bash" => {
            let raw = stringify_first_truthy(args, &["command"], "");
            let command = slice_utf16(&normalize(&raw), 50);
            js_format!("[bash: {}{}]", command, if raw.len() > 50 { "..." } else { "" })
        }
        "grep" | "find" | "ls" => {
            let path = shorten_path(&stringify_first_truthy(args, &["path"], "."));
            if name == "ls" {
                return js_format!("[ls: {}]", path);
            }
            let pattern = stringify_first_truthy(args, &["pattern"], "");
            if name == "grep" {
                js_format!("[grep: /{}/ in {}]", pattern, path)
            } else {
                js_format!("[find: {} in {}]", pattern, path)
            }
        }
        _ => {
            let serialized = JsString::from(js_json::stringify_lossless(args));
            let short = slice_utf16(&serialized, 40);
            js_format!(
                "[{}: {}{}]",
                name,
                short,
                if serialized.len() > 40 { "..." } else { "" }
            )
        }
    }
}

fn format_label_timestamp(timestamp: &JsString, now_millis: i64) -> JsString {
    let now = local_date_fields(now_millis);
    let millis = date_parse_utf16(timestamp.units());
    let parsed = millis.is_finite().then_some(millis as i64);
    let Some(date) = parsed.and_then(local_date_fields) else {
        // Every getter of an invalid `Date` is NaN, and NaN never compares equal.
        return JsString::from("aN/NaN/NaN NaN:NaN");
    };
    let time = format!("{:02}:{:02}", date.hours, date.minutes);
    let same_year = now.is_some_and(|now| now.year == date.year);
    if same_year && now.is_some_and(|now| now.month == date.month && now.day == date.day) {
        return time.into();
    }
    let month = date.month + 1;
    let day = date.day;
    if same_year {
        return js_format!("{}/{} {}", month, day, time);
    }
    let full_year = date.year.to_string();
    let year = &full_year[full_year.len().saturating_sub(2)..];
    js_format!("{}/{}/{} {}", year, month, day, time)
}

fn entry_message(entry: &SessionEntry) -> Option<Cow<'_, AgentMessage>> {
    entry.message()
}

fn assistant_message(entry: &SessionEntry) -> Option<Cow<'_, AssistantMessage>> {
    match entry_message(entry)? {
        Cow::Borrowed(AgentMessage::Assistant(message)) => Some(Cow::Borrowed(message.as_ref())),
        Cow::Owned(AgentMessage::Assistant(message)) => Some(Cow::Owned(*message)),
        _ => None,
    }
}

fn has_text_content(message: &AssistantMessage) -> bool {
    message.content.iter().any(|content| match content {
        AssistantContent::Text(text) => !text.text.trim().is_empty(),
        _ => false,
    })
}

fn is_settings_entry(entry: &SessionEntry) -> bool {
    [
        "label",
        "custom",
        "model_change",
        "thinking_level_change",
        "session_info",
    ]
    .iter()
    .any(|tag| entry.entry_type() == *tag)
}

/// Tree list component with selection and ASCII art visualization.
pub struct TreeList {
    flat_nodes: Vec<FlatNode>,
    filtered_indices: Vec<usize>,
    selected_index: f64,
    current_leaf_id: Option<JsString>,
    max_visible_lines: f64,
    filter_mode: FilterMode,
    search_query: JsString,
    tool_call_map: HashMap<String, ToolCallInfo>,
    multiple_roots: bool,
    show_label_timestamps: bool,
    active_path_ids: HashSet<JsString>,
    visible_parent_map: HashMap<JsString, Option<JsString>>,
    visible_children_map: HashMap<Option<JsString>, Vec<JsString>>,
    last_selected_id: Option<JsString>,
    folded_nodes: HashSet<JsString>,
    is_compacted_user_view: bool,
    pub on_select: Option<Rc<dyn Fn(JsString)>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_label_edit: Option<LabelEditHandler>,
}

impl TreeList {
    pub fn new(
        tree: Vec<SessionTreeNode>,
        current_leaf_id: Option<JsString>,
        max_visible_lines: f64,
        initial_selected_id: Option<JsString>,
        initial_filter_mode: Option<FilterMode>,
    ) -> Self {
        let multiple_roots = tree.len() > 1;
        let mut list = Self {
            flat_nodes: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0.0,
            current_leaf_id,
            max_visible_lines,
            filter_mode: initial_filter_mode.unwrap_or(FilterMode::Default),
            search_query: JsString::default(),
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
            on_label_edit: None,
        };
        list.flat_nodes = list.flatten_tree(&tree);
        list.build_active_path();
        list.apply_filter();

        let target_id = initial_selected_id.or_else(|| list.current_leaf_id.clone());
        list.selected_index = list.find_nearest_visible_index(target_id.as_ref()) as f64;
        list.last_selected_id = list.selected_flat_index().map(|index| {
            list.flat_nodes[*index]
                .node
                .entry
                .id()
                .cloned()
                .unwrap_or_default()
                .to_owned()
        });
        list
    }

    fn find_nearest_visible_index(&self, entry_id: Option<&JsString>) -> usize {
        if self.filtered_indices.is_empty() {
            return 0;
        }
        let entry_map: HashMap<JsString, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().cloned().unwrap_or_default(), index))
            .collect();
        let visible_id_to_index: HashMap<JsString, usize> = self
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(index, flat_index)| {
                (
                    self.flat_nodes[*flat_index]
                        .node
                        .entry
                        .id()
                        .cloned()
                        .unwrap_or_default(),
                    index,
                )
            })
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
        let Some(current_leaf_id) = self.current_leaf_id.as_ref().filter(|id| !id.is_empty()) else {
            return;
        };
        let entry_map: HashMap<JsString, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().cloned().unwrap_or_default().to_owned(), index))
            .collect();

        let mut current_id = Some(current_leaf_id.clone());
        while let Some(id) = current_id.filter(|id| !id.is_empty()) {
            self.active_path_ids.insert(id.clone());
            let Some(node_index) = entry_map.get(&id) else {
                break;
            };
            current_id = self.flat_nodes[*node_index].node.entry.parent_id().cloned();
        }
    }

    fn flatten_tree(&mut self, roots: &[SessionTreeNode]) -> Vec<FlatNode> {
        let mut result = Vec::new();
        self.tool_call_map.clear();

        let leaf_id = self.current_leaf_id.clone();
        let mut contains_active: HashMap<*const SessionTreeNode, bool> = HashMap::new();
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
                let mut has = leaf_id.as_ref() == node.entry.id();
                for child in &node.children {
                    if contains_active[&std::ptr::from_ref(child)] {
                        has = true;
                    }
                }
                contains_active.insert(std::ptr::from_ref(node), has);
            }
        }

        let multiple_roots = roots.len() > 1;
        let mut ordered_roots: Vec<&SessionTreeNode> = roots.iter().collect();
        pi_js_compat::array_sort::js_array_sort(&mut ordered_roots, |left, right| {
            let left_active = contains_active[&std::ptr::from_ref(*left)];
            let right_active = contains_active[&std::ptr::from_ref(*right)];
            f64::from(u8::from(right_active)) - f64::from(u8::from(left_active))
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
                                arguments: JsValue::Object(tool_call.arguments.clone()),
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
                if contains_active[&std::ptr::from_ref(child)] {
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
        let leaf = match self.current_leaf_id.as_ref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        let context_entries = build_context_entries(&entries, leaf);
        if !context_entries.iter().any(|entry| entry.entry_type() == "compaction") {
            return ((0..self.flat_nodes.len()).collect(), false);
        }

        let nodes_by_id: HashMap<JsString, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().cloned().unwrap_or_default().to_owned(), index))
            .collect();
        let nodes = context_entries
            .iter()
            .filter_map(|entry| nodes_by_id.get(&entry.id().cloned().unwrap_or_default()).copied())
            .collect();
        (nodes, true)
    }

    fn should_hide_tool_only_assistant(&self, flat_node: &FlatNode) -> bool {
        let entry = &flat_node.node.entry;
        let Some(message) = assistant_message(entry) else {
            return false;
        };
        if entry.id() == self.current_leaf_id.as_ref() {
            return false;
        }
        let has_text = has_text_content(&message);
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

    fn matches_search(&self, flat_node: &FlatNode, search_tokens: &[JsString]) -> bool {
        if search_tokens.is_empty() {
            return true;
        }
        let node_text = self.get_searchable_text(&flat_node.node).to_lowercase();
        search_tokens.iter().all(|token| node_text.contains(token))
    }

    fn apply_filter(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.last_selected_id = self
                .selected_flat_index()
                .map(|index| {
                    self.flat_nodes[*index]
                        .node
                        .entry
                        .id()
                        .cloned()
                        .unwrap_or_default()
                        .to_owned()
                })
                .or_else(|| self.last_selected_id.clone());
        }

        let search_tokens: Vec<JsString> = split_js_whitespace(&self.search_query.to_lowercase())
            .into_iter()
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
            let mut skip_set: HashSet<JsString> = HashSet::new();
            for flat in &self.flat_nodes {
                let id = flat.node.entry.id().cloned().unwrap_or_default().to_owned();
                let parent_id = flat.node.entry.parent_id();
                if let Some(parent_id) = parent_id
                    && (self.folded_nodes.contains(parent_id) || skip_set.contains(parent_id))
                {
                    skip_set.insert(id);
                }
            }
            filtered.retain(|index| {
                !skip_set.contains(&self.flat_nodes[*index].node.entry.id().cloned().unwrap_or_default())
            });
        }
        self.filtered_indices = filtered;

        self.recalculate_visual_structure();

        if let Some(last_selected_id) = self.last_selected_id.clone().filter(|id| !id.is_empty()) {
            self.selected_index = self.find_nearest_visible_index(Some(&last_selected_id)) as f64;
        } else if self.selected_index >= self.filtered_indices.len() as f64 {
            self.selected_index = self.filtered_indices.len().saturating_sub(1) as f64;
        }

        if !self.filtered_indices.is_empty() {
            self.last_selected_id = self
                .selected_flat_index()
                .map(|index| {
                    self.flat_nodes[*index]
                        .node
                        .entry
                        .id()
                        .cloned()
                        .unwrap_or_default()
                        .to_owned()
                })
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
        let visible_ids: HashSet<JsString> = self
            .filtered_indices
            .iter()
            .map(|index| {
                self.flat_nodes[*index]
                    .node
                    .entry
                    .id()
                    .cloned()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        let entry_map: HashMap<JsString, usize> = self
            .flat_nodes
            .iter()
            .enumerate()
            .map(|(index, flat)| (flat.node.entry.id().cloned().unwrap_or_default().to_owned(), index))
            .collect();

        let find_visible_ancestor = |node_id: &JsString| -> Option<JsString> {
            let mut current_id = self
                .flat_nodes
                .get(*entry_map.get(node_id)?)
                .and_then(|flat| flat.node.entry.parent_id())
                .cloned();
            while let Some(id) = current_id {
                if visible_ids.contains(&id) {
                    return Some(id);
                }
                current_id = entry_map
                    .get(&id)
                    .and_then(|index| self.flat_nodes[*index].node.entry.parent_id())
                    .cloned();
            }
            None
        };

        let mut visible_parent: HashMap<JsString, Option<JsString>> = HashMap::new();
        let mut visible_children: HashMap<Option<JsString>, Vec<JsString>> = HashMap::new();
        visible_children.insert(None, Vec::new());

        for index in &self.filtered_indices {
            let node_id = self.flat_nodes[*index]
                .node
                .entry
                .id()
                .cloned()
                .unwrap_or_default()
                .to_owned();
            let ancestor_id = find_visible_ancestor(&node_id);
            visible_parent.insert(node_id.clone(), ancestor_id.clone());
            visible_children.entry(ancestor_id).or_default().push(node_id);
        }

        let visible_root_ids = visible_children.get(&None).cloned().unwrap_or_default();
        self.multiple_roots = visible_root_ids.len() > 1;

        type StackItem = (JsString, usize, bool, bool, bool, Vec<GutterInfo>, bool);
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
        let entry_ids: Vec<JsString> = self
            .filtered_indices
            .iter()
            .map(|index| {
                self.flat_nodes[*index]
                    .node
                    .entry
                    .id()
                    .cloned()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        let mut visible_children: HashMap<Option<JsString>, Vec<JsString>> = HashMap::new();
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
            visible_children.insert(
                Some(flat.node.entry.id().cloned().unwrap_or_default().to_owned()),
                Vec::new(),
            );
        }
        self.visible_children_map = visible_children;
    }

    fn get_searchable_text(&self, node: &SessionTreeNode) -> JsString {
        let entry = &node.entry;
        let mut parts: Vec<JsString> = Vec::new();
        if let Some(label) = &node.label {
            parts.push(label.clone());
        }

        match entry.known().as_deref() {
            Some(KnownSessionEntry::Message(message)) => {
                let message = message.message.as_ref();
                parts.push(message.role().to_owned());
                parts.push(slice_utf16(&extract_message_full_content(message), 200));
                if message.role() == "bashExecution"
                    && let AgentMessage::Custom(object) = message
                    && let Some(command) = object.get("command").and_then(JsValue::as_string)
                {
                    parts.push(command.to_owned());
                }
            }
            Some(KnownSessionEntry::CustomMessage(entry)) => {
                parts.push(entry.custom_type.clone());
                parts.push(match &entry.content {
                    CustomMessageContent::Text(text) => text.clone(),
                    CustomMessageContent::Blocks(_) => {
                        slice_utf16(&extract_custom_message_content(&entry.content), 200)
                    }
                });
            }
            Some(KnownSessionEntry::Compaction(entry)) => {
                parts.push(JsString::from("compaction"));
                parts.push(entry.summary.clone());
            }
            Some(KnownSessionEntry::BranchSummary(entry)) => {
                parts.push(JsString::from("branch summary"));
                parts.push(entry.summary.clone());
            }
            Some(KnownSessionEntry::SessionInfo(entry)) => {
                parts.push(JsString::from("title"));
                if let Some(name) = &entry.name {
                    parts.push(name.clone());
                }
            }
            Some(KnownSessionEntry::ModelChange(entry)) => {
                parts.push(JsString::from("model"));
                parts.push(entry.model_id.clone());
            }
            Some(KnownSessionEntry::ThinkingLevelChange(entry)) => {
                parts.push(JsString::from("thinking"));
                parts.push(entry.thinking_level.clone());
            }
            Some(KnownSessionEntry::Custom(entry)) => {
                parts.push(JsString::from("custom"));
                parts.push(entry.custom_type.clone());
            }
            Some(KnownSessionEntry::Label(entry)) => {
                parts.push(JsString::from("label"));
                parts.push(entry.label.clone().unwrap_or_default());
            }
            None => {}
        }

        JsString::join(parts, " ")
    }

    pub fn get_search_query(&self) -> &JsString {
        &self.search_query
    }

    fn selected_flat_index(&self) -> Option<&usize> {
        let index = self.selected_index;
        if !index.is_finite() || index < 0.0 || index.fract() != 0.0 {
            return None;
        }
        self.filtered_indices.get(index as usize)
    }

    pub fn selected_entry_id(&self) -> Option<&JsString> {
        self.selected_flat_index()
            .and_then(|index| self.flat_nodes[*index].node.entry.id())
    }

    pub fn get_selected_node(&self) -> Option<&SessionTreeNode> {
        self.selected_flat_index().map(|index| &self.flat_nodes[*index].node)
    }

    pub fn update_node_label(
        &mut self,
        entry_id: &JsString,
        label: Option<&JsString>,
        label_timestamp: Option<&JsString>,
    ) {
        for flat in &mut self.flat_nodes {
            if flat.node.entry.id() == Some(entry_id) {
                flat.node.label = label.cloned();
                flat.node.label_timestamp = label.filter(|label| !label.is_empty()).map(|_| {
                    label_timestamp
                        .cloned()
                        .unwrap_or_else(|| crate::utils::iso_time::now_iso_string().into())
                });
                break;
            }
        }
    }

    fn get_status_labels(&self) -> JsString {
        let mut labels = JsString::default();
        labels.push_str(self.filter_mode.label());
        if self.show_label_timestamps {
            labels.push_str(" [+label time]");
        }
        labels
    }

    fn get_entry_display_text(&self, node: &SessionTreeNode, is_selected: bool) -> JsString {
        let entry = &node.entry;
        let result = match entry.known().as_deref() {
            Some(KnownSessionEntry::Message(message)) => {
                let message = message.message.as_ref();
                match message.role() {
                    role if role == "user" => {
                        with_theme(|theme| theme.fg(ThemeColor::Accent, "user: "))
                            + &normalize(&slice_utf16(&extract_message_full_content(message), 200))
                    }
                    role if role == "assistant" => {
                        let message = assistant_message(entry).expect("assistant entry");
                        let text_content = normalize(&slice_utf16(&extract_assistant_content(&message), 200));
                        if !text_content.is_empty() {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: ")) + &text_content
                        } else if message.stop_reason == StopReason::Aborted {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Muted, "(aborted)"))
                        } else if let Some(error_message) =
                            message.error_message.as_ref().filter(|text| !text.is_empty())
                        {
                            let error = slice_utf16(&normalize(&error_message.clone()), 80);
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Error, &error))
                        } else {
                            with_theme(|theme| theme.fg(ThemeColor::Success, "assistant: "))
                                + &with_theme(|theme| theme.fg(ThemeColor::Muted, "(no content)"))
                        }
                    }
                    role if role == "toolResult" => {
                        let tool_call = match message {
                            AgentMessage::ToolResult(result) if !result.tool_call_id.is_empty() => {
                                self.tool_call_map.get(&result.tool_call_id)
                            }
                            _ => None,
                        };
                        if let Some(tool_call) = tool_call {
                            with_theme(|theme| {
                                theme.fg(
                                    ThemeColor::Muted,
                                    format_tool_call(&tool_call.name, &tool_call.arguments),
                                )
                            })
                        } else {
                            let tool_name = match message {
                                AgentMessage::ToolResult(result) => result.tool_name.clone(),
                                _ => "tool".to_owned(),
                            };
                            with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("[{}]", tool_name)))
                        }
                    }
                    role if role == "bashExecution" => {
                        let command = match message {
                            AgentMessage::Custom(object) => object
                                .get("command")
                                .and_then(JsValue::as_string)
                                .cloned()
                                .unwrap_or_default(),
                            _ => JsString::default(),
                        };
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("[bash]: {}", (&normalize(&command)))))
                    }
                    role => with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("[{}]", (&role)))),
                }
            }
            Some(KnownSessionEntry::CustomMessage(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::CustomMessageLabel, &js_format!("[{}]: ", entry.custom_type)))
                    + &normalize(&extract_custom_message_content(&entry.content))
            }
            Some(KnownSessionEntry::Compaction(entry)) => {
                let tokens = js_round(entry.tokens_before / 1000.0);
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::BorderAccent,
                        &js_format!(
                            "[compaction: {}k tokens]: ",
                            pi_js_compat::number::js_number_to_string(tokens)
                        ),
                    )
                }) + &normalize(&entry.summary)
            }
            Some(KnownSessionEntry::BranchSummary(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Warning, "[branch summary]: ")) + &normalize(&entry.summary)
            }
            Some(KnownSessionEntry::ModelChange(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("[model: {}]", entry.model_id)))
            }
            Some(KnownSessionEntry::ThinkingLevelChange(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("[thinking: {}]", entry.thinking_level)))
            }
            Some(KnownSessionEntry::Custom(entry)) => {
                with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("[custom: {}]", entry.custom_type)))
            }
            Some(KnownSessionEntry::Label(entry)) => with_theme(|theme| {
                theme.fg(
                    ThemeColor::Dim,
                    &js_format!("[label: {}]", entry.label.clone().unwrap_or_else(|| "(cleared)".into())),
                )
            }),
            Some(KnownSessionEntry::SessionInfo(entry)) => match entry.name.as_ref().filter(|name| !name.is_empty()) {
                Some(name) => with_theme(|theme| {
                    theme.fg(ThemeColor::Dim, "[title: ")
                        + &theme.fg(ThemeColor::Dim, name)
                        + &theme.fg(ThemeColor::Dim, "]")
                }),
                None => with_theme(|theme| {
                    theme.fg(ThemeColor::Dim, "[title: ")
                        + &theme.italic(theme.fg(ThemeColor::Dim, "empty"))
                        + &theme.fg(ThemeColor::Dim, "]")
                }),
            },
            _ => JsString::default(),
        };

        if is_selected {
            with_theme(|theme| theme.bold(&result))
        } else {
            result
        }
    }

    fn is_foldable(&self, entry_id: &JsString) -> bool {
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

    fn find_branch_segment_start(&self, direction: BranchDirection) -> f64 {
        let Some(selected_id) = self.selected_entry_id().filter(|id| !id.is_empty()).cloned() else {
            return self.selected_index;
        };
        let indices: HashMap<JsString, usize> = self
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(index, flat)| {
                (
                    self.flat_nodes[*flat].node.entry.id().cloned().expect("tree entry id"),
                    index,
                )
            })
            .collect();
        let mut current_id = selected_id;
        if direction == BranchDirection::Down {
            loop {
                let children = self
                    .visible_children_map
                    .get(&Some(current_id.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if children.is_empty() {
                    return indices[&current_id] as f64;
                }
                if children.len() > 1 {
                    return indices[&children[0]] as f64;
                }
                current_id = children[0].clone();
            }
        }
        loop {
            let Some(parent) = self.visible_parent_map.get(&current_id).cloned().flatten() else {
                return indices[&current_id] as f64;
            };
            let children = self
                .visible_children_map
                .get(&Some(parent.clone()))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let segment_start = indices[&current_id] as f64;
            if children.len() > 1 && segment_start < self.selected_index {
                return segment_start;
            }
            current_id = parent;
        }
    }

    pub fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") {
            self.selected_index = if self.selected_index == 0.0 {
                self.filtered_indices.len() as f64 - 1.0
            } else {
                self.selected_index - 1.0
            };
        } else if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = if self.selected_index == self.filtered_indices.len() as f64 - 1.0 {
                0.0
            } else {
                self.selected_index + 1.0
            };
        } else if keybindings.matches(key_data, "app.tree.foldOrUp") {
            let current_id = self.selected_entry_id().filter(|id| !id.is_empty()).cloned();
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
            let current_id = self.selected_entry_id().filter(|id| !id.is_empty()).cloned();
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
            self.selected_index = js_math_max(0.0, self.selected_index - self.max_visible_lines);
        } else if keybindings.matches(key_data, "tui.editor.cursorRight")
            || keybindings.matches(key_data, "tui.select.pageDown")
        {
            self.selected_index = js_math_min(
                self.filtered_indices.len() as f64 - 1.0,
                self.selected_index + self.max_visible_lines,
            );
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(entry_id) = self.selected_entry_id().cloned()
                && let Some(on_select) = &self.on_select
            {
                on_select(entry_id);
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            if !self.search_query.is_empty() {
                self.search_query = JsString::default();
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
                self.search_query = self.search_query.slice(0, self.search_query.len().saturating_sub(1));
                self.folded_nodes.clear();
                self.apply_filter();
            }
        } else if keybindings.matches(key_data, "app.tree.editLabel") {
            if let Some(node) = self.get_selected_node()
                && let Some(on_label_edit) = &self.on_label_edit
            {
                on_label_edit(
                    &node.entry.id().cloned().unwrap_or_default(),
                    node.label.clone().as_ref(),
                );
            }
        } else if keybindings.matches(key_data, "app.tree.toggleLabelTimestamp") {
            self.show_label_timestamps = !self.show_label_timestamps;
        } else {
            let has_control_chars = key_data.units().iter().copied().any(|character| {
                let code = u32::from(character);
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
    fn handle_input(&mut self, data: &JsString) {
        TreeList::handle_input(self, data);
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = Vec::new();

        if self.filtered_indices.is_empty() {
            lines.push(truncate(
                &with_theme(|theme| theme.fg(ThemeColor::Muted, "  No entries found")),
                width,
                "...",
            ));
            lines.push(truncate(
                &with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("  (0/0){}", self.get_status_labels()))),
                width,
                "...",
            ));
            return lines;
        }

        let start = js_math_max(
            0.0,
            js_math_min(
                self.selected_index - (self.max_visible_lines / 2.0).floor(),
                self.filtered_indices.len() as f64 - self.max_visible_lines,
            ),
        );
        let end = js_math_min(start + self.max_visible_lines, self.filtered_indices.len() as f64);
        let range = if start.is_nan() || end.is_nan() {
            0..0
        } else {
            start as usize..end as usize
        };

        let mut rendered_rows = Vec::new();
        for i in range {
            let flat_index = self.filtered_indices[i];
            let flat_node = &self.flat_nodes[flat_index];
            let entry = &flat_node.node.entry;
            let is_selected = i as f64 == self.selected_index;
            let entry_id = entry.id().cloned().unwrap_or_default();

            let cursor = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
            } else {
                JsString::from("  ")
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
            let is_folded = self.folded_nodes.contains(&entry_id);
            let mut prefix_chars = JsString::default();
            for character_index in 0..total_chars {
                let level = character_index / 3;
                let position_in_level = character_index % 3;
                let gutter = flat_node.gutters.iter().find(|gutter| gutter.position == level);
                if let Some(gutter) = gutter {
                    if position_in_level == 0 {
                        prefix_chars.push_str(if gutter.show { '│' } else { ' ' });
                    } else {
                        prefix_chars.push_str(' ');
                    }
                } else if connector_position == Some(level) {
                    if position_in_level == 0 {
                        prefix_chars.push_str(if flat_node.is_last { '└' } else { '├' });
                    } else if position_in_level == 1 {
                        let foldable = self.is_foldable(&entry_id);
                        prefix_chars.push_str(if is_folded {
                            '⊞'
                        } else if foldable {
                            '⊟'
                        } else {
                            '─'
                        });
                    } else {
                        prefix_chars.push_str(' ');
                    }
                } else {
                    prefix_chars.push_str(' ');
                }
            }

            let shows_fold_in_connector = flat_node.show_connector && !flat_node.is_virtual_root_child;
            let fold_marker = if is_folded && !shows_fold_in_connector {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "⊞ "))
            } else {
                JsString::default()
            };
            let path_marker = if self.active_path_ids.contains(&entry_id) {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "• "))
            } else {
                JsString::default()
            };
            let label = flat_node
                .node
                .label
                .as_ref()
                .filter(|label| !label.is_empty())
                .map(|label| with_theme(|theme| theme.fg(ThemeColor::Warning, &js_format!("[{}] ", label))))
                .unwrap_or_default();
            let label_timestamp = if self.show_label_timestamps {
                flat_node
                    .node
                    .label
                    .as_ref()
                    .filter(|label| !label.is_empty())
                    .zip(
                        flat_node
                            .node
                            .label_timestamp
                            .as_ref()
                            .filter(|timestamp| !timestamp.is_empty()),
                    )
                    .map(|(_, timestamp)| {
                        with_theme(|theme| {
                            theme.fg(
                                ThemeColor::Muted,
                                &js_format!("{} ", format_label_timestamp(timestamp, pi_ai::request::now_millis())),
                            )
                        })
                    })
                    .unwrap_or_default()
            } else {
                JsString::default()
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
                    &js_format!(
                        "  ({}/{}){}",
                        pi_js_compat::number::js_number_to_string(self.selected_index + 1.0),
                        self.filtered_indices.len(),
                        self.get_status_labels()
                    ),
                )
            }),
            width,
            "...",
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let query = self.tree_list.borrow().get_search_query().to_owned();
        if query.is_empty() {
            return vec![truncate(
                &js_format!(
                    "  {}",
                    with_theme(|theme| theme.fg(ThemeColor::Muted, "Type to search:"))
                ),
                width,
                "...",
            )];
        }
        vec![truncate(
            &js_format!(
                "  {} {}",
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Type to search:")),
                with_theme(|theme| theme.fg(ThemeColor::Accent, &query))
            ),
            width,
            "...",
        )]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}

const TREE_HELP_ITEMS: [(&[&str], &str, bool); 7] = [
    (&["tui.select.up", "tui.select.down"], "move", false),
    (&["tui.editor.cursorLeft", "tui.editor.cursorRight"], "page", false),
    (&["app.tree.foldOrUp", "app.tree.unfoldOrDown"], "branch", false),
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

fn help_replacement_pattern() -> &'static JsRegex {
    static PATTERN: OnceLock<JsRegex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        JsRegex::new(r"\bpageUp\b|\bpageDown\b|\bup\b|\bdown\b|\bleft\b|\bright\b").expect("static help regex")
    })
}

fn format_help_keys(keybindings: &[&str]) -> JsString {
    let manager = get_keybindings();
    let keys: Vec<JsString> = keybindings
        .iter()
        .filter_map(|binding| manager.get_keys(*binding).first().cloned())
        .collect();
    if keys.is_empty() {
        return JsString::default();
    }
    let text = format_key_text(compact_raw_keys(&keys), Default::default());
    help_replacement_pattern().replace_all_with(&text, |captures| {
        let matched = captures.get(0).expect("whole help-key match").text();
        for (key, replacement) in [
            ("pageUp", "pgup"),
            ("pageDown", "pgdn"),
            ("up", "↑"),
            ("down", "↓"),
            ("left", "←"),
            ("right", "→"),
        ] {
            if matched == key {
                return replacement.into();
            }
        }
        matched
    })
}

fn compact_raw_keys(keys: &[JsString]) -> JsString {
    if keys.len() == 1 {
        return keys[0].clone();
    }
    let parts: Vec<(JsString, JsString)> = keys
        .iter()
        .map(|key| match key.units().iter().rposition(|unit| *unit == 0x2b) {
            Some(index) => (key.slice(0, index + 1), key.slice(index + 1, key.len())),
            None => (JsString::default(), key.clone()),
        })
        .collect();
    let prefix = &parts[0].0;
    if !prefix.is_empty() && parts.iter().all(|part| &part.0 == prefix) {
        return js_format!("{}{}", prefix, JsString::join(parts.iter().map(|part| &part.1), "/"));
    }
    JsString::join(keys, "/")
}

/// Component that renders tree help as semantic rows with chunk-aware wrapping.
pub struct TreeHelp;

impl Component for TreeHelp {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let items: Vec<JsString> = TREE_HELP_ITEMS
            .iter()
            .map(|(keys, label, label_first)| {
                let text = format_help_keys(keys);
                if text.is_empty() {
                    JsString::from(*label)
                } else if *label_first {
                    js_format!("{} {}", label, text)
                } else {
                    js_format!("{} {}", text, label)
                }
            })
            .collect();

        let available_width = width.max(1);
        let indent = "  ";
        let separator = " · ";
        let mut lines: Vec<JsString> = Vec::new();
        let mut current_line = JsString::default();

        for item in items {
            let candidate = if current_line.is_empty() {
                let indented = js_format!("{}{}", indent, item);
                if visible_width(&indented) <= available_width {
                    indented
                } else {
                    item.clone()
                }
            } else {
                js_format!("{}{}{}", current_line, separator, item)
            };
            if current_line.is_empty() || visible_width(&candidate) <= available_width {
                current_line = candidate;
                continue;
            }

            lines.extend(wrap_text_with_ansi(current_line.trim_end(), available_width));
            let indented = js_format!("{}{}", indent, item);
            current_line = if visible_width(&indented) <= available_width {
                indented
            } else {
                item
            };
        }

        if !current_line.is_empty() {
            lines.extend(wrap_text_with_ansi(current_line.trim_end(), available_width));
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
    entry_id: JsString,
    pub on_submit: Option<LabelEditHandler>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    focused: bool,
}

impl LabelInput {
    pub fn new(entry_id: &JsString, current_label: Option<&JsString>) -> Self {
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
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = Vec::new();
        let indent = "  ";
        let available_width = width.saturating_sub(indent.len());
        lines.push(truncate(
            &js_format!(
                "{}{}",
                indent,
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Label (empty to remove):"))
            ),
            width,
            "...",
        ));
        for line in self.input.render(available_width) {
            lines.push(truncate(&js_format!("{}{}", indent, line), width, "..."));
        }
        lines.push(truncate(
            &js_format!(
                "{}{}  {}",
                indent,
                key_hint("tui.select.confirm", "save"),
                key_hint("tui.select.cancel", "cancel")
            ),
            width,
            "...",
        ));
        lines
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.confirm") {
            let value = self.input.get_value().trim();
            let label = (!value.is_empty()).then_some(value);
            if let Some(on_submit) = &self.on_submit {
                on_submit(&self.entry_id, label.as_ref());
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

type LabelInputHandle = Rc<RefCell<Option<Rc<RefCell<LabelInput>>>>>;

fn hide_label_input(
    input: &LabelInputHandle,
    label_container: &Rc<RefCell<Container>>,
    tree_container: &Rc<RefCell<Container>>,
    tree_list: &Rc<RefCell<TreeList>>,
) {
    *input.borrow_mut() = None;
    label_container.borrow_mut().clear();
    tree_container.borrow_mut().clear();
    tree_container.borrow_mut().add_child(tree_list.clone());
}

/// Component that renders a session tree selector for navigation.
pub struct TreeSelectorComponent {
    tree_list: Rc<RefCell<TreeList>>,
    label_input: LabelInputHandle,
    label_input_container: Rc<RefCell<Container>>,
    tree_container: Rc<RefCell<Container>>,
    on_label_change: Option<LabelEditHandler>,
    focused: Rc<Cell<bool>>,
    container: Container,
}

/// Initial selection and filter for the tree dialog.
pub struct TreeSelectorOptions {
    pub initial_selected_id: Option<JsString>,
    pub initial_filter_mode: Option<FilterMode>,
}

impl TreeSelectorComponent {
    pub fn new(
        tree: Vec<SessionTreeNode>,
        current_leaf_id: Option<JsString>,
        terminal_height: f64,
        on_select: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        on_label_change: Option<LabelEditHandler>,
        options: TreeSelectorOptions,
    ) -> Self {
        let TreeSelectorOptions {
            initial_selected_id,
            initial_filter_mode,
        } = options;
        let max_visible_lines = js_math_max(5.0, (terminal_height / 2.0).floor());
        let is_empty = tree.is_empty();
        let mut tree_list = TreeList::new(
            tree,
            current_leaf_id,
            max_visible_lines,
            initial_selected_id,
            initial_filter_mode,
        );
        tree_list.on_select = Some(on_select);
        tree_list.on_cancel = Some(on_cancel.clone());
        let tree_list = Rc::new(RefCell::new(tree_list));

        let mut component = Self {
            tree_list,
            label_input: Rc::new(RefCell::new(None)),
            label_input_container: Rc::new(RefCell::new(Container::new())),
            tree_container: Rc::new(RefCell::new(Container::new())),
            on_label_change,
            focused: Rc::new(Cell::new(false)),
            container: Container::new(),
        };
        component.wire_label_edit();
        component.rebuild_layout();
        if is_empty {
            tokio::task::spawn_local(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                on_cancel();
            });
        }
        component
    }

    fn wire_label_edit(&mut self) {
        let tree_list = self.tree_list.clone();
        let label_input = self.label_input.clone();
        let label_input_container = self.label_input_container.clone();
        let tree_container = self.tree_container.clone();
        let on_label_change = self.on_label_change.clone();
        let focused = self.focused.clone();
        self.tree_list.borrow_mut().on_label_edit = Some(Rc::new(move |entry_id, current_label| {
            let mut input = LabelInput::new(entry_id, current_label);
            input.set_focused(focused.get());
            let submit_tree_list = tree_list.clone();
            let submit_label_input = label_input.clone();
            let submit_label_container = label_input_container.clone();
            let submit_tree_container = tree_container.clone();
            let submit_label_change = on_label_change.clone();
            input.on_submit = Some(Rc::new(move |id, label| {
                submit_tree_list.borrow_mut().update_node_label(id, label, None);
                if let Some(callback) = &submit_label_change {
                    callback(id, label);
                }
                hide_label_input(
                    &submit_label_input,
                    &submit_label_container,
                    &submit_tree_container,
                    &submit_tree_list,
                );
            }));
            let cancel_tree_list = tree_list.clone();
            let cancel_label_input = label_input.clone();
            let cancel_label_container = label_input_container.clone();
            let cancel_tree_container = tree_container.clone();
            input.on_cancel = Some(Rc::new(move || {
                hide_label_input(
                    &cancel_label_input,
                    &cancel_label_container,
                    &cancel_tree_container,
                    &cancel_tree_list,
                );
            }));
            let input = Rc::new(RefCell::new(input));
            *label_input.borrow_mut() = Some(input.clone());
            tree_container.borrow_mut().clear();
            label_input_container.borrow_mut().clear();
            label_input_container.borrow_mut().add_child(input);
        }));
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

        self.tree_container.borrow_mut().clear();
        self.tree_container.borrow_mut().add_child(self.tree_list.clone());
        self.container.add_child(self.tree_container.clone());
        self.container.add_child(self.label_input_container.clone());
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(dynamic_border()));
    }

    pub fn tree_list(&self) -> &Rc<RefCell<TreeList>> {
        &self.tree_list
    }
}

impl Drop for TreeSelectorComponent {
    fn drop(&mut self) {
        self.tree_list.borrow_mut().on_label_edit = None;
        self.label_input.borrow_mut().take();
        self.label_input_container.borrow_mut().clear();
        self.tree_container.borrow_mut().clear();
    }
}

impl Deref for TreeSelectorComponent {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.container
    }
}
impl DerefMut for TreeSelectorComponent {
    fn deref_mut(&mut self) -> &mut Container {
        &mut self.container
    }
}
impl Component for TreeSelectorComponent {
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, key_data: &JsString) {
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
        self.focused.get()
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused.set(focused);
        if let Some(input) = self.label_input.borrow().as_ref() {
            input.borrow_mut().set_focused(focused);
        }
    }
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;
    #[test]
    fn preview_slice_splits_utf16_before_display_encoding() {
        let text = JsString::from("x😀");
        assert_eq!(slice_utf16(&text, 2).units(), &[0x78, 0xd83d]);
        let raw = JsString::from_utf16(vec![0xd800, 10, 0xfffd]);
        assert_eq!(normalize(&raw).units(), &[0xd800, 32, 0xfffd]);
    }
}
