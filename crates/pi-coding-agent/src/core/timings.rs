//! Startup timing instrumentation (`core/timings.ts`). Enable with `PI_TIMING=1`.

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingLabel {
    Main,
    Extensions,
}

impl TimingLabel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Extensions => "extensions",
        }
    }
}

struct Timing {
    label: String,
    ms: i64,
}

struct TimingNamespace {
    timings: Vec<Timing>,
    last_time: i64,
}

/// Namespaces in first-insertion order, matching JavaScript `Map` iteration.
static NAMESPACES: Mutex<Vec<(TimingLabel, TimingNamespace)>> = Mutex::new(Vec::new());

/// The source reads `PI_TIMING` once at module load; this reads it once on first use.
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("PI_TIMING").is_some_and(|value| value == "1"))
}

fn now_ms() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch");
    i64::try_from(elapsed.as_millis()).expect("millisecond timestamp fits in i64")
}

fn namespaces() -> std::sync::MutexGuard<'static, Vec<(TimingLabel, TimingNamespace)>> {
    NAMESPACES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn reset_namespace(namespaces: &mut Vec<(TimingLabel, TimingNamespace)>, namespace: TimingLabel, now: i64) {
    let fresh = TimingNamespace {
        timings: Vec::new(),
        last_time: now,
    };
    match namespaces.iter_mut().find(|(label, _)| *label == namespace) {
        Some((_, existing)) => *existing = fresh,
        None => namespaces.push((namespace, fresh)),
    }
}

pub fn reset_timings(namespace: TimingLabel) {
    if !enabled() {
        return;
    }
    reset_namespace(&mut namespaces(), namespace, now_ms());
}

pub fn time(label: &str, namespace: TimingLabel) {
    if !enabled() {
        return;
    }
    let now = now_ms();
    let mut namespaces = namespaces();
    if !namespaces.iter().any(|(existing, _)| *existing == namespace) {
        reset_namespace(&mut namespaces, namespace, now_ms());
    }
    let (_, timing_namespace) = namespaces
        .iter_mut()
        .find(|(existing, _)| *existing == namespace)
        .expect("namespace was just ensured");
    timing_namespace.timings.push(Timing {
        label: label.to_owned(),
        ms: now - timing_namespace.last_time,
    });
    timing_namespace.last_time = now;
}

fn format_timing_group(title: &str, timings: &[Timing]) -> Option<String> {
    let printable: Vec<&Timing> = timings.iter().filter(|timing| timing.ms >= 0).collect();
    if printable.is_empty() {
        return None;
    }
    let mut output = format!("\n--- {title} ---\n");
    for timing in &printable {
        output.push_str(&format!("  {}: {}ms\n", timing.label, timing.ms));
    }
    let total: i64 = printable.iter().map(|timing| timing.ms).sum();
    output.push_str(&format!("  TOTAL: {total}ms\n"));
    output.push_str(&format!("{}\n\n", "-".repeat(title.chars().count() + 8)));
    Some(output)
}

/// Print every namespace's timings to stderr.
pub fn print_timings() {
    if !enabled() {
        return;
    }
    for (namespace, timing_namespace) in namespaces().iter() {
        let title = format!("Startup Timings: {}", namespace.as_str());
        if let Some(group) = format_timing_group(&title, &timing_namespace.timings) {
            eprint!("{group}");
        }
    }
}
