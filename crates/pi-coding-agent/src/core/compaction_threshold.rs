//! Optional automatic-compaction percentage (`core/compaction-threshold.ts`).

use pi_js_compat::js_json::{JsValue, to_js_string};
use serde_json::Value;

use super::settings_manager::SettingsFailure;

/// Missing means reserve-token triggering; present values must be finite percentages in `(0, 100]`.
pub fn parse_compaction_threshold_percent(value: Option<&Value>) -> Result<Option<f64>, SettingsFailure> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(percent) = value.as_f64()
        && percent.is_finite() && percent > 0.0 && percent <= 100.0
    {
        return Ok(Some(percent));
    }
    let text = to_js_string(&JsValue::from(value)).map_err(SettingsFailure::new)?.to_utf8_lossy();
    Err(SettingsFailure::new(format!(
        "Invalid compaction.thresholdPercent setting: {text}. Use a finite number greater than 0 and at most 100, or remove the setting."
    )))
}
