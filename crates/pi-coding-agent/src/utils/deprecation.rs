use std::collections::HashSet;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crate::utils::chalk::Chalk;

fn emitted_warnings() -> &'static Mutex<HashSet<String>> {
    static WARNINGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    WARNINGS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Emit a deprecation warning to stderr once per message.
///
/// Returns true when the message was newly emitted.
pub fn warn_deprecation(message: &str) -> bool {
    emit_deprecation_with(&Chalk::stderr(), &mut std::io::stderr(), message)
}

/// Emit a deprecation warning through the supplied chalk instance and writer
/// once per message.
pub fn emit_deprecation_with(chalk: &Chalk, writer: &mut impl Write, message: &str) -> bool {
    if !mark_emitted(message) {
        return false;
    }
    let warning = chalk.yellow(format!("Deprecation warning: {message}"));
    let _ = writeln!(writer, "{}", pi_js_compat::js_json::encode_node_utf8(&warning));
    true
}

/// Clear deprecation warning state.
pub fn clear_deprecation_warnings_for_tests() {
    let mut warnings = emitted_warnings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    warnings.clear();
}

fn mark_emitted(message: &str) -> bool {
    let mut warnings = emitted_warnings()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    warnings.insert(message.to_owned())
}
