//! `pi tools` inventory output (`cli/list-tools.ts`).

use pi_js_compat::collator::locale_compare;
use pi_js_compat::string::utf16_len;

use crate::core::extensions::types::ToolInfo;

const DESCRIPTION_WIDTH: usize = 54;

struct ToolInventoryRow {
    active: &'static str,
    name: String,
    source: String,
    description: String,
}

/// `formatToolInventory`.
pub fn format_tool_inventory(tools: &[ToolInfo], active_tool_names: &[String]) -> String {
    if tools.is_empty() {
        return "Available tools: none".to_owned();
    }

    let mut sorted: Vec<&ToolInfo> = tools.iter().collect();
    sorted.sort_by(|left, right| locale_compare(&left.name, &right.name));
    let rows: Vec<ToolInventoryRow> = sorted
        .into_iter()
        .map(|tool| ToolInventoryRow {
            active: if active_tool_names.iter().any(|name| name == &tool.name) {
                "yes"
            } else {
                "no"
            },
            name: tool.name.clone(),
            source: source_label(tool),
            description: truncate_description(&tool.description),
        })
        .collect();

    let mut lines = vec![format!("Available tools ({})", rows.len()), String::new()];
    lines.extend(format_rows(
        &ToolInventoryRow {
            active: "active",
            name: "tool".to_owned(),
            source: "source".to_owned(),
            description: "description".to_owned(),
        },
        &rows,
    ));
    lines.join("\n")
}

/// `listTools` output including the trailing newline from `console.log`.
pub fn list_tools(tools: &[ToolInfo], active_tool_names: &[String]) -> String {
    format!("{}\n", format_tool_inventory(tools, active_tool_names))
}

fn source_label(tool: &ToolInfo) -> String {
    let source = tool.source_info.source.as_str();
    if source.starts_with("extension:") || source == "sdk" {
        return source.to_owned();
    }
    if tool.source_info.path.starts_with("<builtin:") {
        return "builtin".to_owned();
    }
    source.to_owned()
}

fn truncate_description(description: &str) -> String {
    if utf16_len(description) <= DESCRIPTION_WIDTH {
        return description.to_owned();
    }
    let truncated = slice_utf16(description, DESCRIPTION_WIDTH - 3);
    format!("{}...", js_trim_end(&truncated))
}

/// `String.prototype.slice` over UTF-16 code units; lone surrogates become U+FFFD.
fn slice_utf16(value: &str, end: usize) -> String {
    let units: Vec<u16> = value.encode_utf16().take(end).collect();
    String::from_utf16_lossy(&units)
}

fn js_trim_end(value: &str) -> &str {
    value.trim_end_matches(pi_js_compat::string::is_js_whitespace_char)
}

fn format_rows(headers: &ToolInventoryRow, rows: &[ToolInventoryRow]) -> Vec<String> {
    let widths = [
        row_width(headers.active, rows.iter().map(|row| row.active)),
        row_width(&headers.name, rows.iter().map(|row| row.name.as_str())),
        row_width(&headers.source, rows.iter().map(|row| row.source.as_str())),
        row_width(&headers.description, rows.iter().map(|row| row.description.as_str())),
    ];

    let mut lines = vec![
        join_cells(
            &[
                headers.active.to_owned(),
                headers.name.clone(),
                headers.source.clone(),
                headers.description.clone(),
            ],
            &widths,
        ),
        join_cells(
            &[
                "-".repeat(widths[0]),
                "-".repeat(widths[1]),
                "-".repeat(widths[2]),
                "-".repeat(widths[3]),
            ],
            &widths,
        ),
    ];
    for row in rows {
        lines.push(
            join_cells(
                &[
                    row.active.to_owned(),
                    row.name.clone(),
                    row.source.clone(),
                    row.description.clone(),
                ],
                &widths,
            )
            .trim_end()
            .to_owned(),
        );
    }
    lines.into_iter().map(|line| line.trim_end().to_owned()).collect()
}

fn row_width<'a>(header: &str, values: impl Iterator<Item = &'a str>) -> usize {
    values.fold(utf16_len(header), |width, value| width.max(utf16_len(value)))
}

fn join_cells(cells: &[String; 4], widths: &[usize; 4]) -> String {
    cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| crate::cli::args::pad_end_utf16(cell, *width))
        .collect::<Vec<_>>()
        .join("  ")
}
