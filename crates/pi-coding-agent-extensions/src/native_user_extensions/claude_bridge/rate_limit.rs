//! User-visible Claude subscription rate-limit warnings and failure labels.
use super::stream::string;
use pi_js_compat::js_json::{JsString, JsValue};

#[derive(Default)]
pub(super) struct WarningState {
    step: Option<f64>,
    threshold: Option<JsValue>,
}
impl WarningState {
    pub fn observe(&mut self, info: &JsValue) -> Option<JsString> {
        let status = string(info, "status");
        if status == "allowed" || status == "rejected" {
            self.step = None;
            self.threshold = None;
        }
        if status == "rejected" {
            let reset = info
                .get("resetsAt")
                .and_then(JsValue::as_f64)
                .filter(|v| *v != 0.0)
                .map(local_time)
                .unwrap_or_else(|| "unknown".into());
            let kind = info
                .get("rateLimitType")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            return Some(pi_js_compat::js_format!(
                "Claude rate limited ({}) — resets at {}",
                kind,
                reset
            ));
        }
        if status != "allowed_warning" {
            return None;
        }
        let percent = (info.get("utilization").and_then(JsValue::as_f64).unwrap_or(0.0) * 100.0 + 0.5).floor();
        let step = (percent / 5.0).floor();
        let threshold = info.get("surpassedThreshold").cloned();
        if self.step.is_some_and(|old| step <= old) && self.threshold == threshold {
            return None;
        }
        self.step = Some(step);
        self.threshold = threshold;
        Some(pi_js_compat::js_format!(
            "Claude rate limit warning: {}% used ({})",
            pi_js_compat::number::js_number_to_string(percent),
            string(info, "rateLimitType")
        ))
    }
}
pub(super) fn label(rejection: &JsValue, failure: &JsString) -> JsString {
    let kind = string(rejection, "rateLimitType");
    let kind = if kind.is_empty() {
        JsString::default()
    } else {
        pi_js_compat::js_format!(" ({})", kind)
    };
    let reset = rejection
        .get("resetsAt")
        .and_then(JsValue::as_f64)
        .filter(|v| *v != 0.0);
    let reset = reset
        .map(|value| pi_js_compat::js_format!(" — resets {}", local_time(value)))
        .unwrap_or_default();
    pi_js_compat::js_format!("Claude rate limit{}{}: {}", kind, reset, failure)
}
fn local_time(seconds: f64) -> String {
    pi_js_compat::date::to_locale_string(seconds * 1000.0, pi_js_compat::date::LocaleDateFormat::TimeEnUs)
}
#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn claude_bridge_rate_warnings_dedupe_five_percent_steps_thresholds_and_rearm() {
        let mut state = WarningState::default();
        let frame = |utilization: f64, threshold: f64| {
            JsValue::object([
                ("status", JsValue::String("allowed_warning".into())),
                ("rateLimitType", JsValue::String("five_hour".into())),
                ("utilization", JsValue::Number(utilization)),
                ("surpassedThreshold", JsValue::Number(threshold)),
            ])
        };
        assert_eq!(
            state.observe(&frame(0.81, 0.8)),
            Some("Claude rate limit warning: 81% used (five_hour)".into())
        );
        assert_eq!(state.observe(&frame(0.82, 0.8)), None);
        assert_eq!(
            state.observe(&frame(0.86, 0.8)),
            Some("Claude rate limit warning: 86% used (five_hour)".into())
        );
        assert!(state.observe(&frame(0.82, 0.7)).is_some());
        assert_eq!(
            state.observe(&js_json::parse_lossless(r#"{"status":"allowed"}"#).unwrap()),
            None
        );
        assert!(state.observe(&frame(0.81, 0.8)).is_some());
        assert_eq!(
            state.observe(&js_json::parse_lossless(r#"{"status":"rejected","rateLimitType":"seven_day"}"#).unwrap()),
            Some("Claude rate limited (seven_day) — resets at unknown".into())
        );
        assert!(state.observe(&frame(0.81, 0.8)).is_some());
    }
}
