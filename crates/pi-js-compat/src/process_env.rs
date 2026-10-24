//! Node `process.env` model.
//!
//! Rust edition 2024 makes `std::env::set_var`/`remove_var` unsafe, and this
//! workspace forbids unsafe code. Extensions that mutate `process.env` in the
//! source (for example `run-plan`) record the mutation here instead. Every
//! child-process spawn that must observe the source's `process.env` builds its
//! environment from [`vars`].

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct Overlay {
    /// Insertion order, mirroring Node's property order for new keys.
    order: Vec<String>,
    values: HashMap<String, Option<String>>,
}

fn overlay() -> &'static Mutex<Overlay> {
    static OVERLAY: OnceLock<Mutex<Overlay>> = OnceLock::new();
    OVERLAY.get_or_init(|| Mutex::new(Overlay::default()))
}

fn lock() -> std::sync::MutexGuard<'static, Overlay> {
    overlay().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Native environment names cannot be empty or contain NUL/equals.
fn native_environment_name(name: &str) -> Option<&str> {
    (!name.is_empty() && !name.contains(['\0', '='])).then_some(name)
}

/// Native `process.env[name]`: one overlay authority; decode OS bytes at the Node boundary.
pub fn get_native(name: &str) -> Option<crate::js_json::JsString> {
    let name = native_environment_name(name)?;
    let override_value = lock().values.get(name).cloned();
    match override_value {
        Some(value) => value.map(crate::js_json::JsString::from),
        None => std::env::var_os(name).map(|value| crate::node_fs::decode_node_path(&value)),
    }
}

/// Scalar convenience for existing callers; environment values have crossed the OS boundary.
pub fn get(name: &str) -> Option<String> {
    get_native(name).map(|value| {
        value
            .to_utf8()
            .expect("OS environment values decode to Unicode scalars")
    })
}

/// `process.env[name] = value`.
pub fn set(name: &str, value: &str) {
    let Some(name) = native_environment_name(name) else {
        return;
    };
    let mut overlay = lock();
    if !overlay.values.contains_key(name) {
        overlay.order.push(name.to_owned());
    }
    overlay.values.insert(name.to_owned(), Some(value.to_owned()));
}

/// `delete process.env[name]`.
pub fn remove(name: &str) {
    let Some(name) = native_environment_name(name) else {
        return;
    };
    let mut overlay = lock();
    if !overlay.values.contains_key(name) {
        overlay.order.push(name.to_owned());
    }
    overlay.values.insert(name.to_owned(), None);
}

/// Drop every override. Test support only; the source has no equivalent.
pub fn clear() {
    let mut overlay = lock();
    overlay.order.clear();
    overlay.values.clear();
}

/// Snapshot of `process.env` with overrides applied.
///
/// Existing keys keep `std::env` order; keys introduced by [`set`] are appended
/// in insertion order. Tombstoned keys are omitted. Like Node, entries whose
/// key is not valid UTF-8 are dropped and values are decoded lossily (each
/// invalid byte becomes U+FFFD).
pub fn vars() -> Vec<(String, String)> {
    let process: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            Some((name, value.to_string_lossy().into_owned()))
        })
        .collect();
    let overlay = lock();
    let mut result = Vec::with_capacity(process.len() + overlay.order.len());
    for (name, value) in &process {
        match overlay.values.get(name) {
            Some(Some(value)) => result.push((name.clone(), value.clone())),
            Some(None) => {}
            None => result.push((name.clone(), value.clone())),
        }
    }
    let process_names: std::collections::HashSet<&str> = process.iter().map(|(name, _)| name.as_str()).collect();
    for name in &overlay.order {
        if process_names.contains(name.as_str()) {
            continue;
        }
        if let Some(Some(value)) = overlay.values.get(name) {
            result.push((name.clone(), value.clone()));
        }
    }
    result
}
