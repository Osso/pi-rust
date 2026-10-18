//! Port of `packages/coding-agent/src/core/telemetry.ts`.

use crate::core::settings_manager::SettingsManager;

/// `isTruthyEnvFlag`: `"1"`, `"true"`, or `"yes"` (case-insensitive).
fn is_truthy_env_flag(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    if value.is_empty() {
        return false;
    }
    value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
}

/// Whether install telemetry is enabled.
///
/// `telemetry_env` mirrors the TypeScript default parameter: `None` reads
/// `PI_TELEMETRY` from the environment, `Some` uses the provided value.
pub fn is_install_telemetry_enabled(settings: &SettingsManager, telemetry_env: Option<&str>) -> bool {
    match telemetry_env {
        Some(value) => is_truthy_env_flag(Some(value)),
        None => {
            let value = std::env::var("PI_TELEMETRY").ok();
            match value {
                Some(value) => is_truthy_env_flag(Some(&value)),
                None => settings.get_enable_install_telemetry(),
            }
        }
    }
}
