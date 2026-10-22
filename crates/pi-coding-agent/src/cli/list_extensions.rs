//! `pi extensions` inventory output (`cli/list-extensions.ts`).

use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

use crate::core::extensions::ExtensionState;
use crate::core::extensions::types::Extension;
use crate::utils::paths::node_basename;

const FIRST_PARTY_PREFIX: &str = "<first-party:";

struct ExtensionInventoryRow {
    scope: JsString,
    source: JsString,
    extension: JsString,
    commands: JsString,
    tools: JsString,
    handlers: JsString,
}

impl ExtensionInventoryRow {
    fn cells(&self) -> [&JsString; 6] {
        [
            &self.scope,
            &self.source,
            &self.extension,
            &self.commands,
            &self.tools,
            &self.handlers,
        ]
    }
}

/// `formatExtensionInventory`.
pub fn format_extension_inventory(extensions: &[Extension]) -> JsString {
    if extensions.is_empty() {
        return "Loaded extensions: none".into();
    }

    let mut sorted: Vec<&Extension> = extensions.iter().collect();
    sorted.sort_by(|left, right| locale_compare_utf16(left.read().path.units(), right.read().path.units()));
    let rows: Vec<ExtensionInventoryRow> = sorted
        .into_iter()
        .map(|extension| {
            let registration = extension.read();
            ExtensionInventoryRow {
                scope: scope_label(&registration),
                source: source_label(&registration),
                extension: extension_label(&registration),
                commands: registration.commands.len().to_string().into(),
                tools: registration.tools.len().to_string().into(),
                handlers: registration.handlers.len().to_string().into(),
            }
        })
        .collect();

    let mut lines = vec![js_format!("Loaded extensions ({})", rows.len()), JsString::default()];
    lines.extend(format_rows(&headers(), &rows));
    JsString::join(lines, "\n")
}

/// `listExtensions` output including the trailing newline from `console.log`.
pub fn list_extensions(extensions: &[Extension]) -> JsString {
    format_extension_inventory(extensions) + "\n"
}

fn is_first_party_path(path: &JsString) -> bool {
    path.starts_with(FIRST_PARTY_PREFIX) && path.ends_with(">")
}

fn extension_label(extension: &ExtensionState) -> JsString {
    if is_first_party_path(&extension.path) {
        return extension.path.slice(FIRST_PARTY_PREFIX.len(), extension.path.len() - 1);
    }
    node_basename(&extension.path)
}

fn scope_label(extension: &ExtensionState) -> JsString {
    if is_first_party_path(&extension.path) {
        return "runtime".into();
    }
    extension.source_info.scope.as_str().into()
}

fn source_label(extension: &ExtensionState) -> JsString {
    if is_first_party_path(&extension.path) {
        return "built-in".into();
    }
    extension.source_info.source.clone()
}

fn headers() -> ExtensionInventoryRow {
    ExtensionInventoryRow {
        scope: "scope".into(),
        source: "source".into(),
        extension: "extension".into(),
        commands: "commands".into(),
        tools: "tools".into(),
        handlers: "handlers".into(),
    }
}

fn format_rows(headers: &ExtensionInventoryRow, rows: &[ExtensionInventoryRow]) -> Vec<JsString> {
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

fn join_cells(cells: [&JsString; 6], widths: &[usize; 6]) -> JsString {
    JsString::join(cells.iter().zip(widths).map(|(cell, width)| cell.pad_end(*width)), "  ")
}
