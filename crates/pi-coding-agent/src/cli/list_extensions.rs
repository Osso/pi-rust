//! `pi extensions` inventory output (`cli/list-extensions.ts`).

use std::path::Path;

use pi_js_compat::collator::locale_compare;
use pi_js_compat::string::utf16_len;

use crate::core::extensions::types::Extension;

const FIRST_PARTY_PREFIX: &str = "<first-party:";

struct ExtensionInventoryRow {
    scope: String,
    source: String,
    extension: String,
    commands: String,
    tools: String,
    handlers: String,
}

/// `formatExtensionInventory`.
pub fn format_extension_inventory(extensions: &[Extension]) -> String {
    if extensions.is_empty() {
        return "Loaded extensions: none".to_owned();
    }

    let mut sorted: Vec<&Extension> = extensions.iter().collect();
    sorted.sort_by(|left, right| locale_compare(&left.read().path, &right.read().path));
    let rows: Vec<ExtensionInventoryRow> = sorted
        .into_iter()
        .map(|extension| ExtensionInventoryRow {
            scope: scope_label(extension),
            source: source_label(extension),
            extension: extension_label(extension),
            commands: extension.read().commands.len().to_string(),
            tools: extension.read().tools.len().to_string(),
            handlers: extension.read().handlers.len().to_string(),
        })
        .collect();

    let mut lines = vec![format!("Loaded extensions ({})", rows.len()), String::new()];
    lines.extend(format_rows(&headers(), &rows));
    lines.join("\n")
}

/// `listExtensions` output including the trailing newline from `console.log`.
pub fn list_extensions(extensions: &[Extension]) -> String {
    format!("{}\n", format_extension_inventory(extensions))
}

fn is_first_party_path(path: &str) -> bool {
    path.starts_with(FIRST_PARTY_PREFIX) && path.ends_with('>')
}

fn extension_label(extension: &Extension) -> String {
    let extension = extension.read();
    if is_first_party_path(&extension.path) {
        let start = FIRST_PARTY_PREFIX.len();
        return extension.path[start..extension.path.len() - 1].to_owned();
    }
    basename(&extension.path)
}

fn scope_label(extension: &Extension) -> String {
    let extension = extension.read();
    if is_first_party_path(&extension.path) {
        return "runtime".to_owned();
    }
    extension.source_info.scope.as_str().to_owned()
}

fn source_label(extension: &Extension) -> String {
    let extension = extension.read();
    if is_first_party_path(&extension.path) {
        return "built-in".to_owned();
    }
    extension.source_info.source.clone()
}

/// `node:path.basename` for the paths extension loaders produce.
fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn headers() -> ExtensionInventoryRow {
    ExtensionInventoryRow {
        scope: "scope".to_owned(),
        source: "source".to_owned(),
        extension: "extension".to_owned(),
        commands: "commands".to_owned(),
        tools: "tools".to_owned(),
        handlers: "handlers".to_owned(),
    }
}

fn format_rows(headers: &ExtensionInventoryRow, rows: &[ExtensionInventoryRow]) -> Vec<String> {
    let widths = [
        row_width(&headers.scope, rows.iter().map(|row| row.scope.as_str())),
        row_width(&headers.source, rows.iter().map(|row| row.source.as_str())),
        row_width(&headers.extension, rows.iter().map(|row| row.extension.as_str())),
        row_width(&headers.commands, rows.iter().map(|row| row.commands.as_str())),
        row_width(&headers.tools, rows.iter().map(|row| row.tools.as_str())),
        row_width(&headers.handlers, rows.iter().map(|row| row.handlers.as_str())),
    ];

    let mut lines = vec![
        join_cells(
            &[
                headers.scope.clone(),
                headers.source.clone(),
                headers.extension.clone(),
                headers.commands.clone(),
                headers.tools.clone(),
                headers.handlers.clone(),
            ],
            &widths,
        ),
        join_cells(
            &[
                "-".repeat(widths[0]),
                "-".repeat(widths[1]),
                "-".repeat(widths[2]),
                "-".repeat(widths[3]),
                "-".repeat(widths[4]),
                "-".repeat(widths[5]),
            ],
            &widths,
        ),
    ];
    for row in rows {
        lines.push(
            join_cells(
                &[
                    row.scope.clone(),
                    row.source.clone(),
                    row.extension.clone(),
                    row.commands.clone(),
                    row.tools.clone(),
                    row.handlers.clone(),
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

fn join_cells(cells: &[String; 6], widths: &[usize; 6]) -> String {
    cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| crate::cli::args::pad_end_utf16(cell, *width))
        .collect::<Vec<_>>()
        .join("  ")
}
