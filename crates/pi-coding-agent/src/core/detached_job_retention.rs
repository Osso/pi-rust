//! Detached artifact retention (`core/detached-job-retention.ts`).

use std::cell::RefCell;
use std::cmp::Ordering;

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

#[derive(Debug, Clone, PartialEq)]
pub struct DetachedArtifactRetentionCandidate {
    pub directory_path: String,
    pub byte_size: f64,
    pub terminal_at: f64,
    pub protected_by_live_reference: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetachedArtifactRetentionPolicy {
    pub now: f64,
    pub max_age: f64,
    pub max_bytes: f64,
}

pub fn select_detached_artifact_directories_to_delete(
    candidates: &[DetachedArtifactRetentionCandidate],
    policy: &DetachedArtifactRetentionPolicy,
) -> Result<Vec<String>, String> {
    validate_retention_input(candidates, policy)?;
    let mut ordered = candidates.to_vec();
    ordered.sort_by(compare_candidates);
    let expired = find_expired_candidates(&ordered, policy);
    Ok(select_expired_and_over_cap_candidates(
        &ordered,
        &expired,
        policy.max_bytes,
    ))
}

fn require_nonnegative_finite(value: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{name} must be a nonnegative finite number"));
    }
    Ok(())
}

fn validate_retention_input(
    candidates: &[DetachedArtifactRetentionCandidate],
    policy: &DetachedArtifactRetentionPolicy,
) -> Result<(), String> {
    require_nonnegative_finite(policy.now, "Retention policy now")?;
    require_nonnegative_finite(policy.max_age, "Retention policy maximum age")?;
    require_nonnegative_finite(policy.max_bytes, "Retention policy maximum bytes")?;
    for candidate in candidates {
        require_nonnegative_finite(
            candidate.byte_size,
            &format!("Artifact {} byte size", candidate.directory_path),
        )?;
        require_nonnegative_finite(
            candidate.terminal_at,
            &format!("Artifact {} terminal timestamp", candidate.directory_path),
        )?;
    }
    Ok(())
}

fn compare_candidates(
    left: &DetachedArtifactRetentionCandidate,
    right: &DetachedArtifactRetentionCandidate,
) -> Ordering {
    left.terminal_at
        .partial_cmp(&right.terminal_at)
        .unwrap_or(Ordering::Equal)
        .then_with(|| compare_paths(&left.directory_path, &right.directory_path))
}

/// `String.prototype.localeCompare`: ICU collation in the host locale, like Node.
fn compare_paths(left: &str, right: &str) -> Ordering {
    COLLATOR.with(|collator| collator.borrow().compare(left, right))
}

thread_local! {
    /// `Collator` holds `Rc` payloads, so it is not `Sync`; one per thread.
    static COLLATOR: RefCell<CollatorBorrowed<'static>> = RefCell::new(build_collator());
}

fn build_collator() -> CollatorBorrowed<'static> {
    let locale: Locale = host_locale()
        .parse()
        .unwrap_or_else(|_| "und".parse().expect("und is a valid locale"));
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(CollatorPreferences::from(&locale), options).unwrap_or_else(|_| {
        Collator::try_new(CollatorPreferences::default(), CollatorOptions::default()).expect("root collator")
    })
}

/// ICU's default locale comes from the environment; so does Node's.
fn host_locale() -> String {
    let raw = std::env::var("LC_ALL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var("LC_COLLATE").ok().filter(|value| !value.is_empty()))
        .or_else(|| std::env::var("LANG").ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "und".to_owned());
    let language = raw.split(['.', '@']).next().unwrap_or("und");
    let normalized = language.replace('_', "-");
    if normalized.is_empty() {
        "und".to_owned()
    } else {
        normalized
    }
}

fn find_expired_candidates(
    candidates: &[DetachedArtifactRetentionCandidate],
    policy: &DetachedArtifactRetentionPolicy,
) -> Vec<DetachedArtifactRetentionCandidate> {
    let oldest_retained_timestamp = policy.now - policy.max_age;
    candidates
        .iter()
        .filter(|candidate| {
            !candidate.protected_by_live_reference && candidate.terminal_at <= oldest_retained_timestamp
        })
        .cloned()
        .collect()
}

fn select_expired_and_over_cap_candidates(
    ordered_candidates: &[DetachedArtifactRetentionCandidate],
    expired_candidates: &[DetachedArtifactRetentionCandidate],
    max_bytes: f64,
) -> Vec<String> {
    let expired_paths: Vec<&str> = expired_candidates
        .iter()
        .map(|candidate| candidate.directory_path.as_str())
        .collect();
    let expired_bytes: f64 = expired_candidates.iter().map(|candidate| candidate.byte_size).sum();
    let total_bytes: f64 = ordered_candidates.iter().map(|candidate| candidate.byte_size).sum();
    let mut retained_bytes = total_bytes - expired_bytes;
    let mut paths_to_delete: Vec<String> = expired_paths.iter().map(|path| (*path).to_owned()).collect();
    for candidate in ordered_candidates {
        if retained_bytes <= max_bytes {
            break;
        }
        if candidate.protected_by_live_reference || expired_paths.contains(&candidate.directory_path.as_str()) {
            continue;
        }
        paths_to_delete.push(candidate.directory_path.clone());
        retained_bytes -= candidate.byte_size;
    }
    paths_to_delete
}
