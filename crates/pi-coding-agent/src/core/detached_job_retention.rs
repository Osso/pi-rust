//! Detached artifact retention (`core/detached-job-retention.ts`).

use pi_js_compat::{js_format, js_json::JsString};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct DetachedArtifactRetentionCandidate {
    pub directory_path: JsString,
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
) -> Result<Vec<JsString>, JsString> {
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

fn require_nonnegative_finite(value: f64, name: impl Into<JsString>) -> Result<(), JsString> {
    let name = name.into();
    if !value.is_finite() || value < 0.0 {
        return Err(js_format!("{} must be a nonnegative finite number", name));
    }
    Ok(())
}

fn validate_retention_input(
    candidates: &[DetachedArtifactRetentionCandidate],
    policy: &DetachedArtifactRetentionPolicy,
) -> Result<(), JsString> {
    require_nonnegative_finite(policy.now, "Retention policy now")?;
    require_nonnegative_finite(policy.max_age, "Retention policy maximum age")?;
    require_nonnegative_finite(policy.max_bytes, "Retention policy maximum bytes")?;
    for candidate in candidates {
        require_nonnegative_finite(
            candidate.byte_size,
            js_format!("Artifact {} byte size", candidate.directory_path),
        )?;
        require_nonnegative_finite(
            candidate.terminal_at,
            js_format!("Artifact {} terminal timestamp", candidate.directory_path),
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
        .then_with(|| {
            pi_js_compat::collator::locale_compare_utf16(left.directory_path.units(), right.directory_path.units())
        })
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
) -> Vec<JsString> {
    let expired_paths: Vec<&JsString> = expired_candidates
        .iter()
        .map(|candidate| &candidate.directory_path)
        .collect();
    let expired_bytes: f64 = expired_candidates.iter().map(|candidate| candidate.byte_size).sum();
    let total_bytes: f64 = ordered_candidates.iter().map(|candidate| candidate.byte_size).sum();
    let mut retained_bytes = total_bytes - expired_bytes;
    let mut paths_to_delete: Vec<JsString> = expired_paths.iter().map(|path| (*path).to_owned()).collect();
    for candidate in ordered_candidates {
        if retained_bytes <= max_bytes {
            break;
        }
        if candidate.protected_by_live_reference || expired_paths.contains(&&candidate.directory_path) {
            continue;
        }
        paths_to_delete.push(candidate.directory_path.clone());
        retained_bytes -= candidate.byte_size;
    }
    paths_to_delete
}
