//! Detached artifact cleanup (`core/detached-job-cleanup.ts`).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use indexmap::IndexMap;

use crate::core::detached_job_retention::{
    DetachedArtifactRetentionCandidate, DetachedArtifactRetentionPolicy, select_detached_artifact_directories_to_delete,
};
use crate::core::multi_agent_store::AgentSnapshot;
use crate::core::session_control_db::list_detached_artifact_agents_updated_at_or_before;

const DETACHED_ARTIFACT_MAX_AGE_MS: f64 = (3 * 24 * 60 * 60 * 1_000) as f64;
const DETACHED_ARTIFACT_MAX_BYTES: f64 = (2_u64 * 1024 * 1024 * 1024) as f64;
const ALL_TERMINAL_AGENTS_CUTOFF: &str = "9999-12-31T23:59:59.999Z";
const NO_BYTE_CAP: f64 = 9_007_199_254_740_991.0;
const DETACHED_OUTPUT_LABELS: [&str; 2] = ["Bash output", "Pyrun output"];
const DELETED_PATH_SUFFIX: &str = " (deleted)";

/// `DetachedJobCleanupResult`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetachedJobCleanupResult {
    pub deleted_bytes: i64,
    pub deleted_directories: Vec<String>,
    pub errors: Vec<String>,
    pub retained_bytes: i64,
    pub skipped_reason: Option<String>,
}

/// `DetachedJobCleanupOptions`.
pub struct DetachedJobCleanupOptions<'a> {
    pub artifact_root: String,
    pub now: Option<i64>,
    pub on_directory_quarantined: Option<&'a dyn Fn(&str)>,
}

/// `cleanupDetachedJobArtifacts`; `Err` models the source's thrown selection error.
pub fn cleanup_detached_job_artifacts(
    control_db_path: &str,
    options: &DetachedJobCleanupOptions<'_>,
) -> Result<DetachedJobCleanupResult, String> {
    let Some(process_references) = read_linux_process_references()? else {
        return Ok(empty_cleanup_result(
            "live process reference inspection requires Linux /proc",
        ));
    };
    let mut errors = Vec::new();
    let candidates = collect_terminal_artifact_candidates(
        control_db_path,
        &options.artifact_root,
        &process_references,
        &mut errors,
        ALL_TERMINAL_AGENTS_CUTOFF,
    )?;
    let paths_to_delete = select_detached_artifact_directories_to_delete(
        &candidates,
        &DetachedArtifactRetentionPolicy {
            max_age: DETACHED_ARTIFACT_MAX_AGE_MS,
            max_bytes: DETACHED_ARTIFACT_MAX_BYTES,
            now: options.now.unwrap_or_else(now_millis) as f64,
        },
    )?;
    Ok(delete_selected_artifact_directories(
        &candidates,
        &paths_to_delete,
        &mut errors,
        options.on_directory_quarantined,
    ))
}

/// `runDetachedJobArtifactCleanup`.
pub fn run_detached_job_artifact_cleanup(control_db_path: &str, artifact_root: Option<&str>, now: Option<i64>) {
    let artifact_root = artifact_root
        .map(str::to_owned)
        .unwrap_or_else(|| crate::config::get_agent_dir().to_string_lossy().into_owned());
    let now = now.unwrap_or_else(now_millis);
    match cleanup_expired_detached_job_artifacts(control_db_path, &artifact_root, now) {
        Ok(mut errors) => {
            match detached_artifact_bytes(&artifact_root, &mut errors) {
                Some(bytes) if bytes as f64 > DETACHED_ARTIFACT_MAX_BYTES => {
                    match cleanup_detached_job_artifacts(
                        control_db_path,
                        &DetachedJobCleanupOptions {
                            artifact_root: artifact_root.clone(),
                            now: Some(now),
                            on_directory_quarantined: None,
                        },
                    ) {
                        Ok(result) => errors.extend(result.errors),
                        Err(error) => errors.push(error),
                    }
                }
                _ => {}
            }
            for error in errors {
                eprintln!("Detached artifact cleanup: {error}");
            }
        }
        Err(error) => eprintln!("Detached artifact cleanup failed: {error}"),
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is after the epoch")
        .as_millis() as i64
}

fn cleanup_expired_detached_job_artifacts(
    control_db_path: &str,
    artifact_root: &str,
    now: i64,
) -> Result<Vec<String>, String> {
    let Some(process_references) = read_linux_process_references()? else {
        return Ok(Vec::new());
    };
    let mut errors = Vec::new();
    let cutoff = crate::utils::iso_time::format_iso_millis(now - DETACHED_ARTIFACT_MAX_AGE_MS as i64);
    let candidates = collect_terminal_artifact_candidates(
        control_db_path,
        artifact_root,
        &process_references,
        &mut errors,
        &cutoff,
    )?;
    let paths_to_delete = select_detached_artifact_directories_to_delete(
        &candidates,
        &DetachedArtifactRetentionPolicy {
            max_age: DETACHED_ARTIFACT_MAX_AGE_MS,
            max_bytes: NO_BYTE_CAP,
            now: now as f64,
        },
    )?;
    delete_selected_artifact_directories(&candidates, &paths_to_delete, &mut errors, None);
    Ok(errors)
}

fn detached_artifact_bytes(artifact_root: &str, errors: &mut Vec<String>) -> Option<i64> {
    read_directory_byte_size(
        &Path::new(artifact_root).join("detached-jobs").to_string_lossy(),
        errors,
    )
}

fn empty_cleanup_result(skipped_reason: &str) -> DetachedJobCleanupResult {
    DetachedJobCleanupResult {
        skipped_reason: Some(skipped_reason.to_owned()),
        ..DetachedJobCleanupResult::default()
    }
}

fn delete_selected_artifact_directories(
    candidates: &[DetachedArtifactRetentionCandidate],
    paths_to_delete: &[String],
    errors: &mut Vec<String>,
    on_directory_quarantined: Option<&dyn Fn(&str)>,
) -> DetachedJobCleanupResult {
    let candidates_by_path: IndexMap<&str, &DetachedArtifactRetentionCandidate> = candidates
        .iter()
        .map(|candidate| (candidate.directory_path.as_str(), candidate))
        .collect();
    let mut deleted_directories = Vec::new();
    let mut deleted_bytes = 0.0;
    for directory_path in paths_to_delete {
        let Some(candidate) = candidates_by_path.get(directory_path.as_str()) else {
            continue;
        };
        if delete_unreferenced_artifact_directory(directory_path, errors, on_directory_quarantined) {
            deleted_directories.push(directory_path.clone());
            deleted_bytes += candidate.byte_size;
        }
    }
    let total_bytes: f64 = candidates.iter().map(|candidate| candidate.byte_size).sum();
    DetachedJobCleanupResult {
        deleted_bytes: deleted_bytes as i64,
        deleted_directories,
        errors: std::mem::take(errors),
        retained_bytes: (total_bytes - deleted_bytes) as i64,
        skipped_reason: None,
    }
}

fn delete_unreferenced_artifact_directory(
    directory_path: &str,
    errors: &mut Vec<String>,
    on_directory_quarantined: Option<&dyn Fn(&str)>,
) -> bool {
    let directory = Path::new(directory_path);
    let name = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let quarantine_path = directory
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".cleanup-{name}-{}", pi_ai::utils::uuid::random_uuid()));
    let quarantine = quarantine_path.to_string_lossy().into_owned();
    match fs::symlink_metadata(directory) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            errors.push(format!(
                "Refused to delete non-directory detached artifact path: {directory_path}"
            ));
            return false;
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
        Err(error) => {
            errors.push(format!(
                "Could not delete detached artifact directory {directory_path}: {error}"
            ));
            return false;
        }
    }
    if let Err(error) = fs::rename(directory, &quarantine_path) {
        if error.kind() == io::ErrorKind::NotFound {
            return false;
        }
        errors.push(format!(
            "Could not delete detached artifact directory {directory_path}: {error}"
        ));
        return false;
    }
    if let Some(on_directory_quarantined) = on_directory_quarantined {
        on_directory_quarantined(&quarantine);
    }
    if quarantined_directory_has_live_reference(&quarantine) {
        let _ = fs::rename(&quarantine_path, directory);
        return false;
    }
    match fs::remove_dir_all(&quarantine_path) {
        Ok(()) => true,
        Err(error) => {
            restore_quarantined_directory(&quarantine_path, directory, errors);
            if error.kind() == io::ErrorKind::NotFound {
                return false;
            }
            errors.push(format!(
                "Could not delete detached artifact directory {directory_path}: {error}"
            ));
            false
        }
    }
}

fn quarantined_directory_has_live_reference(quarantine_path: &str) -> bool {
    let Ok(process_references) = read_linux_process_references() else {
        return true;
    };
    let Some(process_references) = process_references else {
        return true;
    };
    match fs::canonicalize(quarantine_path) {
        Ok(canonical) => directory_has_live_reference(&canonical.to_string_lossy(), &process_references),
        Err(_) => true,
    }
}

fn restore_quarantined_directory(quarantine_path: &Path, directory_path: &Path, errors: &mut Vec<String>) {
    if !quarantine_path.exists() || directory_path.exists() {
        return;
    }
    if let Err(error) = fs::rename(quarantine_path, directory_path) {
        errors.push(format!(
            "Could not restore referenced detached artifact directory {}: {error}",
            directory_path.to_string_lossy()
        ));
    }
}

fn collect_terminal_artifact_candidates(
    control_db_path: &str,
    artifact_root: &str,
    process_references: &HashSet<String>,
    errors: &mut Vec<String>,
    updated_at_cutoff: &str,
) -> Result<Vec<DetachedArtifactRetentionCandidate>, String> {
    let mut candidates_by_path: IndexMap<String, DetachedArtifactRetentionCandidate> = IndexMap::new();
    let records = list_detached_artifact_agents_updated_at_or_before(control_db_path, updated_at_cutoff)
        .map_err(|error| error.message().to_owned())?;
    for record in records {
        let Some(candidate) = terminal_artifact_candidate(
            &record.agent,
            &record.session_path,
            artifact_root,
            process_references,
            errors,
        )?
        else {
            continue;
        };
        match candidates_by_path.get_mut(&candidate.directory_path) {
            Some(existing) => existing.protected_by_live_reference = true,
            None => {
                candidates_by_path.insert(candidate.directory_path.clone(), candidate);
            }
        }
    }
    Ok(candidates_by_path.into_values().collect())
}

fn terminal_artifact_candidate(
    agent: &AgentSnapshot,
    session_path: &str,
    artifact_root: &str,
    process_references: &HashSet<String>,
    errors: &mut Vec<String>,
) -> Result<Option<DetachedArtifactRetentionCandidate>, String> {
    if crate::core::multi_agent_store::is_active_lifecycle(agent.lifecycle) {
        return Ok(None);
    }
    let Some(output_path) = detached_output_path(agent) else {
        return Ok(None);
    };
    let Some(directory_path) = detached_artifact_directory(&output_path, session_path, &agent.id, artifact_root)?
    else {
        return Ok(None);
    };
    let Some(terminal_at) = pi_js_compat::date::date_parse_millis(&agent.updated_at) else {
        errors.push(format!(
            "Invalid detached artifact terminal timestamp for {session_path}#{}",
            agent.id
        ));
        return Ok(None);
    };
    let Some(byte_size) = read_directory_byte_size(&directory_path, errors) else {
        return Ok(None);
    };
    Ok(Some(DetachedArtifactRetentionCandidate {
        protected_by_live_reference: directory_has_live_reference(&directory_path, process_references),
        byte_size: byte_size as f64,
        directory_path,
        terminal_at: terminal_at as f64,
    }))
}

fn detached_output_path(agent: &AgentSnapshot) -> Option<String> {
    agent.result.as_ref()?.file_refs.as_ref()?.iter().find_map(|file_ref| {
        file_ref
            .label
            .as_deref()
            .filter(|label| DETACHED_OUTPUT_LABELS.contains(label))
            .map(|_| file_ref.path.clone())
    })
}

fn detached_artifact_directory(
    output_path: &str,
    session_path: &str,
    job_id: &str,
    artifact_root: &str,
) -> Result<Option<String>, String> {
    if !Path::new(output_path).is_absolute() || !is_path_segment(job_id) {
        return Ok(None);
    }
    let directory_path = Path::new(output_path);
    if directory_path.file_name().and_then(|name| name.to_str()) != Some("output.log") {
        return Ok(None);
    }
    let directory_path = resolve_path(directory_path.parent().unwrap_or_else(|| Path::new("/")));
    let Some(session_name) = session_stem(session_path) else {
        return Ok(None);
    };
    if !is_path_segment(&session_name) {
        return Ok(None);
    }
    let expected_directory_path = resolve_path(
        &Path::new(artifact_root)
            .join("detached-jobs")
            .join(&session_name)
            .join(job_id),
    );
    if directory_path != expected_directory_path {
        return Ok(None);
    }
    let metadata = match fs::symlink_metadata(&directory_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let canonical_directory_path = fs::canonicalize(&directory_path).map_err(|error| error.to_string())?;
    let canonical_artifact_root = fs::canonicalize(artifact_root).map_err(|error| error.to_string())?;
    let expected_canonical_path = canonical_artifact_root
        .join("detached-jobs")
        .join(&session_name)
        .join(job_id);
    Ok((canonical_directory_path == expected_canonical_path)
        .then(|| canonical_directory_path.to_string_lossy().into_owned()))
}

fn session_stem(session_path: &str) -> Option<String> {
    Path::new(session_path)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
}

fn is_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && Path::new(value)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            == Some(value.to_owned())
}

/// `path.resolve`: absolute lexical resolution without touching the filesystem.
fn resolve_path(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => resolved.push(prefix.as_os_str()),
            Component::RootDir => resolved.push("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(part) => resolved.push(part),
        }
    }
    resolved
}

fn read_directory_byte_size(directory_path: &str, errors: &mut Vec<String>) -> Option<i64> {
    let mut byte_size = 0_i64;
    let mut pending_directories = vec![PathBuf::from(directory_path)];
    while let Some(current_directory) = pending_directories.pop() {
        let entries = match fs::read_dir(&current_directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                errors.push(format!(
                    "Could not measure detached artifact directory {directory_path}: {error}"
                ));
                return None;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    errors.push(format!(
                        "Could not measure detached artifact directory {directory_path}: {error}"
                    ));
                    return None;
                }
            };
            let entry_path = entry.path();
            let metadata = match fs::symlink_metadata(&entry_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
                Err(error) => {
                    errors.push(format!(
                        "Could not measure detached artifact directory {directory_path}: {error}"
                    ));
                    return None;
                }
            };
            if metadata.is_dir() {
                pending_directories.push(entry_path);
                continue;
            }
            byte_size += metadata.len() as i64;
        }
    }
    Some(byte_size)
}

fn read_linux_process_references() -> Result<Option<HashSet<String>>, String> {
    if !cfg!(target_os = "linux") || !Path::new("/proc").exists() {
        return Ok(None);
    }
    let mut references = HashSet::new();
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(error) if is_expected_process_read_error(&error) => return Ok(Some(references)),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if is_expected_process_read_error(&error) => continue,
            Err(error) => return Err(error.to_string()),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.path().is_dir() || !name.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let process_directory = Path::new("/proc").join(name.as_ref());
        add_link_reference(&mut references, &process_directory.join("cwd"))?;
        add_link_reference(&mut references, &process_directory.join("exe"))?;
        add_descriptor_references(&mut references, &process_directory.join("fd"))?;
    }
    Ok(Some(references))
}

fn add_link_reference(references: &mut HashSet<String>, link_path: &Path) -> Result<(), String> {
    match fs::read_link(link_path) {
        Ok(target) => add_absolute_reference(references, &target.to_string_lossy()),
        Err(error) if is_expected_process_read_error(&error) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn add_descriptor_references(references: &mut HashSet<String>, descriptor_directory: &Path) -> Result<(), String> {
    let entries = match fs::read_dir(descriptor_directory) {
        Ok(entries) => entries,
        Err(error) if is_expected_process_read_error(&error) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    for descriptor in entries {
        let descriptor = match descriptor {
            Ok(descriptor) => descriptor,
            Err(error) if is_expected_process_read_error(&error) => continue,
            Err(error) => return Err(error.to_string()),
        };
        add_link_reference(references, &descriptor.path())?;
    }
    Ok(())
}

fn add_absolute_reference(references: &mut HashSet<String>, value: &str) -> Result<(), String> {
    let path = value.strip_suffix(DELETED_PATH_SUFFIX).unwrap_or(value);
    if !Path::new(path).is_absolute() {
        return Ok(());
    }
    match fs::canonicalize(path) {
        Ok(canonical) => {
            references.insert(canonical.to_string_lossy().into_owned());
            Ok(())
        }
        Err(error) if is_error_code(&error, 36) => Ok(()),
        Err(error) if is_expected_reference_resolution_error(&error) => {
            references.insert(resolve_path(Path::new(path)).to_string_lossy().into_owned());
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn directory_has_live_reference(directory_path: &str, references: &HashSet<String>) -> bool {
    let nested_prefix = format!("{directory_path}/");
    references
        .iter()
        .any(|reference| reference == directory_path || reference.starts_with(&nested_prefix))
}

fn is_expected_reference_resolution_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(13 | 2 | 20 | 1))
}

fn is_expected_process_read_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(13 | 2 | 1 | 3))
}

fn is_error_code(error: &io::Error, code: i32) -> bool {
    error.raw_os_error() == Some(code)
}
