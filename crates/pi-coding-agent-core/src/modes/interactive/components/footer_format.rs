use crate::utils::paths::{node_is_absolute, node_resolve, relative_path};
use pi_js_compat::{
    js_format,
    js_json::JsString,
    number::{js_number_to_string, to_fixed},
};

/// Format token counts for compact footer display.
pub fn format_tokens(count: f64) -> String {
    if count < 1000.0 {
        return js_number_to_string(count);
    }
    if count < 10_000.0 {
        return format!("{}k", to_fixed(count / 1000.0, 1));
    }
    if count < 1_000_000.0 {
        return format!("{}k", js_number_to_string((count / 1000.0).round()));
    }
    if count < 10_000_000.0 {
        return format!("{}M", to_fixed(count / 1_000_000.0, 1));
    }
    format!("{}M", js_number_to_string((count / 1_000_000.0).round()))
}

/// Replace the home directory prefix with `~`, preserving opaque path units.
pub fn format_cwd_for_footer(cwd: &JsString, home: Option<&JsString>) -> JsString {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return cwd.clone();
    };
    let resolved_cwd = node_resolve("", cwd)
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message()));
    let resolved_home = node_resolve("", home)
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message()));
    let relative = relative_path(&resolved_home, &resolved_cwd)
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message()));
    let is_inside =
        relative.is_empty() || (relative != ".." && !relative.starts_with("../") && !node_is_absolute(&relative));
    if !is_inside {
        return cwd.clone();
    }
    if relative.is_empty() {
        "~".into()
    } else {
        js_format!("~/{}", relative)
    }
}
