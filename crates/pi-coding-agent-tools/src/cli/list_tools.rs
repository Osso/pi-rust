//! `pi tools` inventory output (`cli/list-tools.ts`).

use pi_js_compat::collator::locale_compare;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

use crate::core::extensions::types::ToolInfo;

const DESCRIPTION_WIDTH: usize = 54;

struct ToolInventoryRow {
    active: JsString,
    name: JsString,
    source: JsString,
    description: JsString,
}

impl ToolInventoryRow {
    fn cells(&self) -> [&JsString; 4] {
        [&self.active, &self.name, &self.source, &self.description]
    }
}

/// `formatToolInventory`.
pub fn format_tool_inventory(tools: &[ToolInfo], active_tool_names: &[String]) -> JsString {
    if tools.is_empty() {
        return "Available tools: none".into();
    }

    let mut sorted: Vec<&ToolInfo> = tools.iter().collect();
    sorted.sort_by(|left, right| locale_compare(&left.name, &right.name));
    let rows: Vec<ToolInventoryRow> = sorted
        .into_iter()
        .map(|tool| ToolInventoryRow {
            active: if active_tool_names.contains(&tool.name) {
                "yes"
            } else {
                "no"
            }
            .into(),
            name: tool.name.as_str().into(),
            source: source_label(tool),
            description: truncate_description(&tool.description),
        })
        .collect();

    let headers = ToolInventoryRow {
        active: "active".into(),
        name: "tool".into(),
        source: "source".into(),
        description: "description".into(),
    };
    let mut lines = vec![js_format!("Available tools ({})", rows.len()), JsString::default()];
    lines.extend(format_rows(&headers, &rows));
    JsString::join(lines, "\n")
}

/// `listTools` output including the trailing newline from `console.log`.
pub fn list_tools(tools: &[ToolInfo], active_tool_names: &[String]) -> JsString {
    format_tool_inventory(tools, active_tool_names) + "\n"
}

fn source_label(tool: &ToolInfo) -> JsString {
    let source = &tool.source_info.source;
    if source.starts_with("extension:") || *source == *"sdk" {
        return source.clone();
    }
    if tool.source_info.path.starts_with("<builtin:") {
        return "builtin".into();
    }
    source.clone()
}

/// `slice(0, width - 3).trimEnd() + "..."`; a split surrogate pair stays a lone unit, as in JS.
fn truncate_description(description: &JsString) -> JsString {
    if description.len() <= DESCRIPTION_WIDTH {
        return description.clone();
    }
    description.slice(0, DESCRIPTION_WIDTH - 3).trim_end() + "..."
}

fn format_rows(headers: &ToolInventoryRow, rows: &[ToolInventoryRow]) -> Vec<JsString> {
    let mut widths = headers.cells().map(JsString::len);
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row.cells()) {
            *width = (*width).max(cell.len());
        }
    }

    let rule = widths.map(|width| JsString::from("-".repeat(width)));
    let mut lines = vec![
        join_cells(headers.cells(), &widths),
        join_cells(rule.each_ref(), &widths),
    ];
    lines.extend(rows.iter().map(|row| join_cells(row.cells(), &widths).trim_end()));
    lines.into_iter().map(|line| line.trim_end()).collect()
}

fn join_cells(cells: [&JsString; 4], widths: &[usize; 4]) -> JsString {
    JsString::join(cells.iter().zip(widths).map(|(cell, width)| cell.pad_end(*width)), "  ")
}
