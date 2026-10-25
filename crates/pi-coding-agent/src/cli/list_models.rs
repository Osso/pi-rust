//! `--list-models` output (`cli/list-models.ts`).

use pi_ai::types::Model;
use pi_js_compat::js_json::JsString;
use pi_tui::fuzzy::fuzzy_filter;

use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::model_registry::ModelRegistry;
use crate::utils::chalk::Chalk;

/// Runtime surface used by [`list_models`] (the source accepts any duck-typed runtime).
pub trait ListModelsRuntime {
    fn get_error(&self) -> Option<JsString>;
    fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError>;
}

impl ListModelsRuntime for ModelRegistry {
    fn get_error(&self) -> Option<JsString> {
        ModelRegistry::get_error(self)
    }

    fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError> {
        ModelRegistry::get_available(self)
    }
}

/// Console boundary, matching the CLI's existing LoginIo injection pattern.
pub trait ListModelsIo: Send + Sync {
    fn stdout(&self, message: &JsString);
    fn stderr(&self, message: &JsString);
}

pub struct StdioListModelsIo;
impl ListModelsIo for StdioListModelsIo {
    fn stdout(&self, message: &JsString) {
        crate::main_runner::write_stdout(message);
    }
    fn stderr(&self, message: &JsString) {
        crate::main_runner::write_stderr(message);
    }
}

/// `listModels`: console effects occur before a later registry rejection.
pub async fn list_models(
    runtime: &dyn ListModelsRuntime,
    search_pattern: Option<&JsString>,
    io: &dyn ListModelsIo,
) -> Result<(), pi_js_compat::node_error::NativeNodeError> {
    if let Some(error) = runtime.get_error().filter(|error| !error.is_empty()) {
        let warning = Chalk::stdout().yellow(JsString::from("Warning: errors loading models.json:\n") + error);
        io.stderr(&(warning + "\n"));
    }
    let models = runtime.get_available()?;
    if models.is_empty() {
        let message = format_no_models_available_message().map_err(crate::config::ConfigError::into_native_error)?;
        io.stdout(&pi_js_compat::js_format!("{}\n", message));
        return Ok(());
    }
    let filtered = match search_pattern.filter(|pattern| !pattern.is_empty()) {
        Some(pattern) => fuzzy_filter(&models, pattern, |model| {
            vec![pi_js_compat::js_format!("{} {}", model.provider, model.id)]
        }),
        None => models,
    };
    if filtered.is_empty() {
        let pattern = search_pattern.expect("filtering a nonempty catalog requires a search pattern");
        io.stdout(&pi_js_compat::js_format!("No models matching \"{}\"\n", pattern));
        return Ok(());
    }
    for line in render_model_table(filtered) {
        io.stdout(&(JsString::from(line) + "\n"));
    }
    Ok(())
}

fn render_model_table(mut models: Vec<Model>) -> Vec<String> {
    models.sort_by(|left, right| {
        pi_js_compat::collator::locale_compare(&left.provider, &right.provider)
            .then_with(|| pi_js_compat::collator::locale_compare(&left.id, &right.id))
    });

    let rows: Vec<[String; 6]> = models
        .iter()
        .map(|model| {
            [
                model.provider.clone(),
                model.id.clone(),
                format_token_count(model.context_window),
                format_token_count(model.max_tokens),
                yes_no(model.reasoning),
                yes_no(model.input.contains(&pi_ai::types::Modality::Image)),
            ]
        })
        .collect();
    let headers = ["provider", "model", "context", "max-out", "thinking", "images"];
    let widths = column_widths(&headers, &rows);

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(padded_line(&headers.map(str::to_owned), &widths));
    for row in &rows {
        lines.push(padded_line(row, &widths));
    }
    lines
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_owned()
}

fn column_widths(headers: &[&str; 6], rows: &[[String; 6]]) -> [usize; 6] {
    let mut widths = headers.map(pi_js_compat::string::utf16_len);
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(pi_js_compat::string::utf16_len(cell));
        }
    }
    widths
}

fn padded_line(cells: &[String; 6], widths: &[usize; 6]) -> String {
    cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| crate::cli::args::pad_end_utf16(cell, *width))
        .collect::<Vec<_>>()
        .join("  ")
}

/// `formatTokenCount`: `200000 -> "200K"`, `1000000 -> "1M"`.
pub fn format_token_count(count: f64) -> String {
    if count >= 1_000_000.0 {
        return scaled_count(count / 1_000_000.0, "M");
    }
    if count >= 1_000.0 {
        return scaled_count(count / 1_000.0, "K");
    }
    pi_js_compat::number::js_number_to_string(count)
}

fn scaled_count(value: f64, suffix: &str) -> String {
    if value.fract() == 0.0 {
        return format!("{}{suffix}", pi_js_compat::number::js_number_to_string(value));
    }
    format!("{}{suffix}", pi_js_compat::number::to_fixed(value, 1))
}
