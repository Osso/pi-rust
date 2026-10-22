use crate::harness::types::{ExecutionEnv, FileError};
use crate::unicode::normalize_nfd;
use pi_js_compat::js_json::JsString;
use tokio_util::sync::CancellationToken;

/// Normalizes Unicode spaces and strips one leading `@` from a model-supplied path.
pub fn normalize_tool_path(path: &JsString) -> JsString {
    let normalized = JsString::from_utf16(
        path.units()
            .iter()
            .map(|unit| if is_unicode_space(*unit) { 0x20 } else { *unit })
            .collect(),
    );
    normalized.strip_prefix("@").unwrap_or(normalized)
}

/// Resolves a tool path through [`ExecutionEnv::absolute_path`].
pub async fn resolve_tool_path(
    env: &dyn ExecutionEnv,
    path: &JsString,
    signal: Option<&CancellationToken>,
) -> Result<JsString, FileError> {
    env.absolute_path(&normalize_tool_path(path), signal).await
}

/// Resolves a read path, trying source-compatible Unicode and apostrophe variants.
pub async fn resolve_read_tool_path(
    env: &dyn ExecutionEnv,
    path: &JsString,
    signal: Option<&CancellationToken>,
) -> Result<JsString, FileError> {
    let resolved = resolve_tool_path(env, path, signal).await?;
    let mut variants = Vec::new();
    push_variant(&mut variants, resolved.clone());
    push_variant(&mut variants, replace_am_pm(&resolved));
    push_variant(&mut variants, normalize_nfd(&resolved));
    push_variant(&mut variants, replace_apostrophes(&resolved));
    push_variant(&mut variants, replace_apostrophes(&normalize_nfd(&resolved)));
    for variant in variants {
        if env.exists(&variant, signal).await? {
            return Ok(variant);
        }
    }
    Ok(resolved)
}

fn push_variant(variants: &mut Vec<JsString>, candidate: JsString) {
    if !variants.contains(&candidate) {
        variants.push(candidate);
    }
}

fn is_unicode_space(unit: u16) -> bool {
    matches!(unit, 0x00a0 | 0x202f | 0x205f | 0x3000) || (0x2000..=0x200a).contains(&unit)
}

fn replace_am_pm(text: &JsString) -> JsString {
    let units = text.units();
    let mut output = Vec::with_capacity(units.len());
    let mut index = 0;
    while index < units.len() {
        if is_am_pm_marker(units, index) {
            output.extend_from_slice(&[0x202f, units[index + 1], units[index + 2], 0x2e]);
            index += 4;
        } else {
            output.push(units[index]);
            index += 1;
        }
    }
    JsString::from_utf16(output)
}

fn is_am_pm_marker(units: &[u16], index: usize) -> bool {
    if units[index] != 0x20 || index + 3 >= units.len() || units[index + 3] != 0x2e {
        return false;
    }
    matches!(
        (units[index + 1], units[index + 2]),
        (0x41 | 0x61, 0x4d | 0x6d) | (0x50 | 0x70, 0x4d | 0x6d)
    )
}

fn replace_apostrophes(text: &JsString) -> JsString {
    text.replace("'", "’")
}
