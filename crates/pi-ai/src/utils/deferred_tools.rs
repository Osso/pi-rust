use std::collections::{BTreeMap, BTreeSet};

use crate::types::{AssistantContent, Context, Message, Tool};

/// Current tools split into prefix and transcript-loaded definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitDeferredTools {
    pub immediate: Vec<Tool>,
    /// Normalized name to tool, in first-insertion order.
    pub deferred: Vec<(String, Tool)>,
}

/// Split current tools with identity name normalization.
pub fn split_deferred_tools(context: &Context, enabled: bool) -> SplitDeferredTools {
    split_deferred_tools_with(context, enabled, str::to_owned)
}

/// Split current tools into prefix and transcript-loaded definitions.
pub fn split_deferred_tools_with(
    context: &Context,
    enabled: bool,
    normalize_name: impl Fn(&str) -> String,
) -> SplitDeferredTools {
    let unique_tools = unique_tools(context, &normalize_name);
    if !enabled {
        return SplitDeferredTools {
            immediate: unique_tools.into_iter().map(|(_, tool)| tool).collect(),
            deferred: Vec::new(),
        };
    }

    let deferred_names = transcript_loaded_names(context, &normalize_name);
    let mut immediate = Vec::new();
    let mut deferred = Vec::new();
    for (name, tool) in unique_tools {
        if deferred_names.contains(&name) {
            deferred.push((name, tool));
        } else {
            immediate.push(tool);
        }
    }
    SplitDeferredTools { immediate, deferred }
}

fn unique_tools(context: &Context, normalize_name: &impl Fn(&str) -> String) -> Vec<(String, Tool)> {
    let mut unique: Vec<(String, Tool)> = Vec::new();
    let mut positions: BTreeMap<String, usize> = BTreeMap::new();
    for tool in context.tools.iter().flatten() {
        let name = normalize_name(&tool.name);
        match positions.get(&name) {
            Some(&index) => unique[index].1 = tool.clone(),
            None => {
                positions.insert(name.clone(), unique.len());
                unique.push((name, tool.clone()));
            }
        }
    }
    unique
}

fn transcript_loaded_names(context: &Context, normalize_name: &impl Fn(&str) -> String) -> BTreeSet<String> {
    let mut used_names = BTreeSet::new();
    let mut deferred_names = BTreeSet::new();
    for message in &context.messages {
        match message {
            Message::Assistant(assistant) => {
                for block in &assistant.content {
                    if let AssistantContent::ToolCall(call) = block {
                        used_names.insert(normalize_name(&call.name));
                    }
                }
            }
            Message::ToolResult(result) => {
                for name in result.added_tool_names.iter().flatten() {
                    let name = normalize_name(name);
                    if !used_names.contains(&name) {
                        deferred_names.insert(name);
                    }
                }
            }
            Message::User(_) => {}
        }
    }
    deferred_names
}
