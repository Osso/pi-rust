//! `--list-models` output (`cli/list-models.ts`).

use pi_ai::auth::types::AuthOperationOptions;
use pi_ai::request::BoxFuture;
use pi_ai::types::Model;
use pi_tui::fuzzy::fuzzy_filter;
use tokio_util::sync::CancellationToken;

use crate::core::auth_guidance::format_no_models_available_message;
use crate::core::model_runtime::ModelRuntime;
use crate::utils::chalk::Chalk;

/// Runtime surface used by [`list_models`] (the source accepts any duck-typed runtime).
pub trait ListModelsRuntime {
    fn get_error(&self) -> Option<String>;
    fn get_available(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<Model>, String>>;
}

impl ListModelsRuntime for ModelRuntime {
    fn get_error(&self) -> Option<String> {
        ModelRuntime::get_error(self)
    }

    fn get_available(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<Model>, String>> {
        Box::pin(async move {
            ModelRuntime::get_available(self, None, options)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

/// Captured output of one `--list-models` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListModelsOutput {
    /// Warning printed to stderr before listing, already colorized like `chalk.yellow`.
    pub warning: Option<String>,
    /// All stdout lines including their trailing newlines.
    pub stdout: String,
}

/// `listModels`.
pub async fn list_models(
    runtime: &dyn ListModelsRuntime,
    search_pattern: Option<&str>,
    signal: Option<CancellationToken>,
) -> Result<ListModelsOutput, String> {
    let warning = runtime
        .get_error()
        .map(|error| Chalk::stderr().yellow(&format!("Warning: errors loading models.json:\n{error}")));

    let models = runtime.get_available(AuthOperationOptions::new(signal)).await?;
    if models.is_empty() {
        return Ok(ListModelsOutput {
            warning,
            stdout: format!("{}\n", format_no_models_available_message()),
        });
    }

    let filtered = match search_pattern {
        Some(pattern) => fuzzy_filter(&models, pattern, |model| vec![model.provider.clone(), model.id.clone()]),
        None => models,
    };
    if filtered.is_empty() {
        let pattern = search_pattern.unwrap_or_default();
        return Ok(ListModelsOutput {
            warning,
            stdout: format!("No models matching \"{pattern}\"\n"),
        });
    }

    Ok(ListModelsOutput {
        warning,
        stdout: render_model_table(filtered),
    })
}

fn render_model_table(mut models: Vec<Model>) -> String {
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
    lines.join("\n") + "\n"
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
