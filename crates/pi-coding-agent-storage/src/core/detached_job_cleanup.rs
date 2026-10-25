//! Detached artifact cleanup (`core/detached-job-cleanup.ts`).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

use crate::core::detached_job_retention::{
    DetachedArtifactRetentionCandidate, DetachedArtifactRetentionPolicy, select_detached_artifact_directories_to_delete,
};
use crate::core::multi_agent_store::AgentSnapshot;
use crate::core::session_control_db::list_detached_artifact_agents_updated_at_or_before;
use crate::utils::paths::{node_basename, node_dirname, node_extname, node_join, node_resolve};

const DETACHED_ARTIFACT_MAX_AGE_MS: f64 = (3 * 24 * 60 * 60 * 1_000) as f64;
const DETACHED_ARTIFACT_MAX_BYTES: f64 = (2_u64 * 1024 * 1024 * 1024) as f64;
const ALL_TERMINAL_AGENTS_CUTOFF: &str = "9999-12-31T23:59:59.999Z";
const DETACHED_OUTPUT_LABELS: [&str; 2] = ["Bash output", "Pyrun output"];
const DELETED_PATH_SUFFIX: &str = " (deleted)";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetachedJobCleanupResult {
    pub deleted_bytes: i64,
    pub deleted_directories: Vec<JsString>,
    pub errors: Vec<JsString>,
    pub retained_bytes: i64,
    pub skipped_reason: Option<JsString>,
}

pub struct DetachedJobCleanupOptions<'a> {
    pub artifact_root: JsString,
    pub now: Option<f64>,
    pub on_directory_quarantined: Option<&'a dyn Fn(&JsString)>,
}

pub fn cleanup_detached_job_artifacts(
    control_db_path: &JsString,
    options: &DetachedJobCleanupOptions<'_>,
) -> Result<DetachedJobCleanupResult, pi_ai::request::CallbackError> {
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
            now: options.now.unwrap_or_else(now_millis),
        },
    )?;
    delete_selected_artifact_directories(
        &candidates,
        &paths_to_delete,
        &mut errors,
        options.on_directory_quarantined,
    )
}

pub fn run_detached_job_artifact_cleanup(
    control_db_path: &JsString,
    artifact_root: Option<&JsString>,
    now: Option<f64>,
) -> Result<(), crate::config::ConfigError> {
    // `getAgentDir()` is a JS string: Node decodes the env/home path bytes as UTF-8.
    let artifact_root = match artifact_root {
        Some(root) => root.clone(),
        None => crate::config::get_agent_dir()?,
    };
    let now = now.unwrap_or_else(now_millis);
    match cleanup_detached_job_artifacts(
        control_db_path,
        &DetachedJobCleanupOptions {
            artifact_root,
            now: Some(now),
            on_directory_quarantined: None,
        },
    ) {
        Ok(result) => {
            for error in result.errors {
                eprintln!("Detached artifact cleanup: {}", encode_node_utf8(&error));
            }
        }
        Err(error) => eprintln!("Detached artifact cleanup failed: {}", encode_node_utf8(&error.message)),
    }
    Ok(())
}

fn now_millis() -> f64 {
    pi_js_compat::date::js_now_millis()
}

fn empty_cleanup_result(reason: &str) -> DetachedJobCleanupResult {
    DetachedJobCleanupResult {
        skipped_reason: Some(reason.into()),
        ..Default::default()
    }
}

fn delete_selected_artifact_directories(
    candidates: &[DetachedArtifactRetentionCandidate],
    paths_to_delete: &[JsString],
    errors: &mut Vec<JsString>,
    on_directory_quarantined: Option<&dyn Fn(&JsString)>,
) -> Result<DetachedJobCleanupResult, pi_ai::request::CallbackError> {
    let candidates_by_path: IndexMap<&JsString, &DetachedArtifactRetentionCandidate> = candidates
        .iter()
        .map(|candidate| (&candidate.directory_path, candidate))
        .collect();
    let mut deleted_directories = Vec::new();
    let mut deleted_bytes = 0.0;
    for directory_path in paths_to_delete {
        let Some(candidate) = candidates_by_path.get(directory_path) else {
            continue;
        };
        if delete_unreferenced_artifact_directory(directory_path, errors, on_directory_quarantined)? {
            deleted_directories.push(directory_path.clone());
            deleted_bytes += candidate.byte_size;
        }
    }
    let total_bytes: f64 = candidates.iter().map(|candidate| candidate.byte_size).sum();
    Ok(DetachedJobCleanupResult {
        deleted_bytes: deleted_bytes as i64,
        deleted_directories,
        errors: std::mem::take(errors),
        retained_bytes: (total_bytes - deleted_bytes) as i64,
        skipped_reason: None,
    })
}

fn delete_unreferenced_artifact_directory(
    directory_path: &JsString,
    errors: &mut Vec<JsString>,
    on_directory_quarantined: Option<&dyn Fn(&JsString)>,
) -> Result<bool, pi_js_compat::node_error::NativeNodeError> {
    let quarantine = node_join(
        node_dirname(directory_path),
        js_format!(
            ".cleanup-{}-{}",
            node_basename(directory_path),
            pi_ai::utils::random_uuid::random_uuid()?
        ),
    );
    match fs::symlink_metadata(encode_node_utf8(directory_path)) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            errors.push(js_format!(
                "Refused to delete non-directory detached artifact path: {}",
                directory_path
            ));
            return Ok(false);
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            errors.push(js_format!(
                "Could not delete detached artifact directory {}: {}",
                directory_path,
                error.to_string()
            ));
            return Ok(false);
        }
    }
    if let Err(error) = fs::rename(encode_node_utf8(directory_path), encode_node_utf8(&quarantine)) {
        if error.kind() == io::ErrorKind::NotFound {
            return Ok(false);
        }
        errors.push(js_format!(
            "Could not delete detached artifact directory {}: {}",
            directory_path,
            error.to_string()
        ));
        return Ok(false);
    }
    if let Some(on_directory_quarantined) = on_directory_quarantined {
        on_directory_quarantined(&quarantine);
    }
    if quarantined_directory_has_live_reference(&quarantine) {
        let _ = fs::rename(encode_node_utf8(&quarantine), encode_node_utf8(directory_path));
        return Ok(false);
    }
    match fs::remove_dir_all(encode_node_utf8(&quarantine)) {
        Ok(()) => Ok(true),
        Err(error) => {
            restore_quarantined_directory(&quarantine, directory_path, errors);
            if error.kind() == io::ErrorKind::NotFound {
                return Ok(false);
            }
            errors.push(js_format!(
                "Could not delete detached artifact directory {}: {}",
                directory_path,
                error.to_string()
            ));
            Ok(false)
        }
    }
}

fn quarantined_directory_has_live_reference(quarantine: &JsString) -> bool {
    let Ok(Some(references)) = read_linux_process_references() else {
        return true;
    };
    match fs::canonicalize(encode_node_utf8(quarantine)) {
        Ok(canonical) => directory_has_live_reference(&filesystem_path_text(&canonical), &references),
        Err(_) => true,
    }
}

fn restore_quarantined_directory(quarantine: &JsString, directory: &JsString, errors: &mut Vec<JsString>) {
    if !Path::new(&encode_node_utf8(quarantine)).exists() || Path::new(&encode_node_utf8(directory)).exists() {
        return;
    }
    if let Err(error) = fs::rename(encode_node_utf8(quarantine), encode_node_utf8(directory)) {
        errors.push(js_format!(
            "Could not restore referenced detached artifact directory {}: {}",
            directory,
            error.to_string()
        ));
    }
}

fn collect_terminal_artifact_candidates(
    control_db_path: &JsString,
    artifact_root: &JsString,
    references: &HashSet<JsString>,
    errors: &mut Vec<JsString>,
    cutoff: &str,
) -> Result<Vec<DetachedArtifactRetentionCandidate>, JsString> {
    let mut candidates_by_path: IndexMap<JsString, DetachedArtifactRetentionCandidate> = IndexMap::new();
    let records = list_detached_artifact_agents_updated_at_or_before(control_db_path, cutoff)
        .map_err(|error| error.message().clone())?;
    for record in records {
        let Some(candidate) =
            terminal_artifact_candidate(&record.agent, &record.session_path, artifact_root, references, errors)?
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
    session_path: &JsString,
    artifact_root: &JsString,
    references: &HashSet<JsString>,
    errors: &mut Vec<JsString>,
) -> Result<Option<DetachedArtifactRetentionCandidate>, JsString> {
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
    let terminal_at = pi_js_compat::date::date_parse_utf16(agent.updated_at.units());
    if !terminal_at.is_finite() {
        errors.push(js_format!(
            "Invalid detached artifact terminal timestamp for {}#{}",
            session_path,
            agent.id
        ));
        return Ok(None);
    }
    let Some(byte_size) = read_directory_byte_size(&directory_path, errors) else {
        return Ok(None);
    };
    Ok(Some(DetachedArtifactRetentionCandidate {
        protected_by_live_reference: directory_has_live_reference(&directory_path, references),
        byte_size: byte_size as f64,
        directory_path,
        terminal_at,
    }))
}

fn detached_output_path(agent: &AgentSnapshot) -> Option<JsString> {
    agent.result.as_ref()?.file_refs.as_ref()?.iter().find_map(|reference| {
        reference
            .label
            .as_ref()
            .filter(|label| DETACHED_OUTPUT_LABELS.iter().any(|expected| *label == expected))
            .map(|_| reference.path.clone())
    })
}

fn detached_artifact_directory(
    output_path: &JsString,
    session_path: &JsString,
    job_id: &JsString,
    artifact_root: &JsString,
) -> Result<Option<JsString>, JsString> {
    if !output_path.starts_with("/") || !is_path_segment(job_id) || node_basename(output_path) != "output.log" {
        return Ok(None);
    }
    let directory_path = resolve_path(&node_dirname(output_path))?;
    let name = node_basename(session_path);
    let session_name = name.slice(0, name.len() - node_extname(&name).len());
    if !is_path_segment(&session_name) {
        return Ok(None);
    }
    let session_root = node_dirname(session_path);
    let mut owning_root = None;
    for root in [artifact_root, &session_root] {
        let expected = resolve_path(&node_join(
            node_join(node_join(root, "detached-jobs"), &session_name),
            job_id,
        ))?;
        if directory_path == expected {
            owning_root = Some(root);
            break;
        }
    }
    let Some(owning_root) = owning_root.filter(|root| !root.is_empty()) else {
        return Ok(None);
    };
    let metadata = match fs::symlink_metadata(encode_node_utf8(&directory_path)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string().into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let canonical_directory =
        fs::canonicalize(encode_node_utf8(&directory_path)).map_err(|error| JsString::from(error.to_string()))?;
    let canonical_root =
        fs::canonicalize(encode_node_utf8(owning_root)).map_err(|error| JsString::from(error.to_string()))?;
    let canonical_directory = filesystem_path_text(&canonical_directory);
    let expected_canonical = node_join(
        node_join(
            node_join(filesystem_path_text(&canonical_root), "detached-jobs"),
            &session_name,
        ),
        job_id,
    );
    Ok((canonical_directory == expected_canonical).then_some(canonical_directory))
}

fn is_path_segment(value: &JsString) -> bool {
    !value.is_empty() && value != "." && value != ".." && node_basename(value) == *value
}

fn resolve_path(path: &JsString) -> Result<JsString, JsString> {
    node_resolve("", path).map_err(|error| error.message().clone())
}

/// OS-returned UTF-8 filenames are decoded only after the filesystem boundary.
fn filesystem_path_text(path: &Path) -> JsString {
    path.to_string_lossy().into_owned().into()
}

fn read_directory_byte_size(directory: &JsString, errors: &mut Vec<JsString>) -> Option<i64> {
    let mut bytes = 0_i64;
    let mut pending = vec![PathBuf::from(encode_node_utf8(directory))];
    while let Some(current) = pending.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                errors.push(js_format!(
                    "Could not measure detached artifact directory {}: {}",
                    directory,
                    error.to_string()
                ));
                return None;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    errors.push(js_format!(
                        "Could not measure detached artifact directory {}: {}",
                        directory,
                        error.to_string()
                    ));
                    return None;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
                Err(error) => {
                    errors.push(js_format!(
                        "Could not measure detached artifact directory {}: {}",
                        directory,
                        error.to_string()
                    ));
                    return None;
                }
            };
            if metadata.is_dir() {
                pending.push(path);
            } else {
                bytes += metadata.len() as i64;
            }
        }
    }
    Some(bytes)
}

pub fn read_linux_process_references() -> Result<Option<HashSet<JsString>>, JsString> {
    if !cfg!(target_os = "linux") || !Path::new("/proc").exists() {
        return Ok(None);
    }
    let mut references = HashSet::new();
    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(error) if is_expected_process_read_error(&error) => return Ok(Some(references)),
        Err(error) => return Err(error.to_string().into()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if is_expected_process_read_error(&error) => continue,
            Err(error) => return Err(error.to_string().into()),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.path().is_dir() || !name.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let directory = Path::new("/proc").join(name.as_ref());
        add_link_reference(&mut references, &directory.join("cwd"))?;
        add_link_reference(&mut references, &directory.join("exe"))?;
        add_descriptor_references(&mut references, &directory.join("fd"))?;
    }
    Ok(Some(references))
}

fn add_link_reference(references: &mut HashSet<JsString>, path: &Path) -> Result<(), JsString> {
    match fs::read_link(path) {
        Ok(target) => add_absolute_reference(references, &filesystem_path_text(&target)),
        Err(error) if is_expected_process_read_error(&error) => Ok(()),
        Err(error) => Err(error.to_string().into()),
    }
}

fn add_descriptor_references(references: &mut HashSet<JsString>, directory: &Path) -> Result<(), JsString> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if is_expected_process_read_error(&error) => return Ok(()),
        Err(error) => return Err(error.to_string().into()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if is_expected_process_read_error(&error) => continue,
            Err(error) => return Err(error.to_string().into()),
        };
        add_link_reference(references, &entry.path())?;
    }
    Ok(())
}

fn add_absolute_reference(references: &mut HashSet<JsString>, value: &JsString) -> Result<(), JsString> {
    let path = value.strip_suffix(DELETED_PATH_SUFFIX).unwrap_or_else(|| value.clone());
    if !path.starts_with("/") {
        return Ok(());
    }
    match fs::canonicalize(encode_node_utf8(&path)) {
        Ok(canonical) => {
            references.insert(filesystem_path_text(&canonical));
            Ok(())
        }
        Err(error) if error.raw_os_error() == Some(36) => Ok(()),
        Err(error) if is_expected_reference_resolution_error(&error) => {
            references.insert(resolve_path(&path)?);
            Ok(())
        }
        Err(error) => Err(error.to_string().into()),
    }
}

fn directory_has_live_reference(directory: &JsString, references: &HashSet<JsString>) -> bool {
    let prefix = directory.clone() + "/";
    references
        .iter()
        .any(|reference| reference == directory || reference.starts_with(&prefix))
}

fn is_expected_reference_resolution_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(13 | 2 | 20 | 1))
}
fn is_expected_process_read_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(13 | 2 | 1 | 3))
}
