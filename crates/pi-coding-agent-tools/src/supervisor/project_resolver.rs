//! Supervisor project resolution (`supervisor/project-resolver.ts`).

use std::collections::HashSet;
use std::path::Path;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_fs::node_read_error;
use pi_js_compat::string::is_js_whitespace_unit;
use serde::ser::SerializeMap;

use crate::utils::child_process::{SpawnSyncOptions, spawn_sync};
use crate::utils::paths::{node_basename, node_join, node_resolve};

pub const DEFAULT_SUPERVISOR_KB_DIR: &str = "/syncthing/Sync/KB";

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SupervisorProjectMapping {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remotes: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<Vec<JsString>>,
}

/// Project mappings in JavaScript `Object.entries` order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupervisorProjectConfig(Vec<(JsString, SupervisorProjectMapping)>);

impl SupervisorProjectConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, project_id: impl Into<JsString>, mapping: SupervisorProjectMapping) {
        self.0.push((project_id.into(), mapping));
    }

    pub fn get(&self, project_id: &JsString) -> Option<&SupervisorProjectMapping> {
        self.0
            .iter()
            .find(|(existing, _)| existing == project_id)
            .map(|(_, mapping)| mapping)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&JsString, &SupervisorProjectMapping)> {
        self.0.iter().map(|(project_id, mapping)| (project_id, mapping))
    }
}

impl serde::Serialize for SupervisorProjectConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (project_id, mapping) in &self.0 {
            map.serialize_entry(project_id, mapping)?;
        }
        map.end()
    }
}

impl FromIterator<(JsString, SupervisorProjectMapping)> for SupervisorProjectConfig {
    fn from_iter<T: IntoIterator<Item = (JsString, SupervisorProjectMapping)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

pub struct ResolveSupervisorProjectInput<'a> {
    pub config: &'a SupervisorProjectConfig,
    pub cwd: &'a JsString,
    pub remote_repository_names: &'a [JsString],
}

const SLASH: u16 = b'/' as u16;
const COLON: u16 = b':' as u16;
const AT: u16 = b'@' as u16;

/// `extractRemoteRepositoryIdentity`.
pub fn extract_remote_repository_identity(remote_url: &JsString) -> Option<JsString> {
    let normalized = strip_scp_authority(&strip_scheme_authority(&strip_git_suffix(&strip_trailing_slashes(
        &remote_url.trim(),
    ))));
    let segments: Vec<JsString> = normalized.split('/').filter(|segment| !segment.is_empty()).collect();
    match segments.len() {
        0 => None,
        1 => segments.into_iter().next(),
        length => Some(JsString::join(segments[length - 2..].iter().cloned(), "/")),
    }
}

/// `extractRemoteRepositoryName`.
pub fn extract_remote_repository_name(remote_url: &JsString) -> Option<JsString> {
    extract_remote_repository_identity(remote_url).map(|identity| last_segment(&identity))
}

/// `.replace(/\/+$/, "")`.
fn strip_trailing_slashes(value: &JsString) -> JsString {
    let end = value
        .units()
        .iter()
        .rposition(|unit| *unit != SLASH)
        .map_or(0, |index| index + 1);
    value.slice(0, end)
}

/// `.replace(/\.git$/, "")`.
fn strip_git_suffix(value: &JsString) -> JsString {
    value.strip_suffix(".git").unwrap_or_else(|| value.clone())
}

/// `.replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]+\//i, "")`.
fn strip_scheme_authority(value: &JsString) -> JsString {
    let units = value.units();
    let is_alpha = |unit: u16| u8::try_from(unit).is_ok_and(|byte| byte.is_ascii_alphabetic());
    let is_scheme_tail =
        |unit: u16| u8::try_from(unit).is_ok_and(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte));
    if !units.first().is_some_and(|unit| is_alpha(*unit)) {
        return value.clone();
    }
    let scheme_end = 1 + units[1..].iter().take_while(|unit| is_scheme_tail(**unit)).count();
    let Some(after_scheme) = value.slice(scheme_end, value.len()).strip_prefix("://") else {
        return value.clone();
    };
    let authority_length = after_scheme.units().iter().take_while(|unit| **unit != SLASH).count();
    if authority_length == 0 || authority_length == after_scheme.len() {
        return value.clone();
    }
    after_scheme.slice(authority_length + 1, after_scheme.len())
}

/// `.replace(/^[^@]+@[^:]+:/, "")`.
fn strip_scp_authority(value: &JsString) -> JsString {
    let units = value.units();
    let Some(at) = units.iter().position(|unit| *unit == AT) else {
        return value.clone();
    };
    let Some(host_length) = units[at + 1..].iter().position(|unit| *unit == COLON) else {
        return value.clone();
    };
    if at == 0 || host_length == 0 {
        return value.clone();
    }
    value.slice(at + 1 + host_length + 1, value.len())
}

/// `resolveSupervisorProjectForCwd`.
pub fn resolve_supervisor_project_for_cwd(cwd: &JsString, kb_dir: &JsString) -> Result<JsString, JsString> {
    let config_path = node_join(
        node_join(node_join(kb_dir.clone(), "memory"), "supervisor"),
        "projects.json",
    );
    let config = read_supervisor_project_config(&config_path)?;
    resolve_supervisor_project(&config, cwd, &read_remote_repository_names(cwd))
}

/// `readSupervisorProjectConfig`. Error messages are the source text; JSON syntax errors
/// come from the lossless `JSON.parse` port.
pub fn read_supervisor_project_config(path: &JsString) -> Result<SupervisorProjectConfig, JsString> {
    let encoded_path = encode_node_utf8(path);
    if !Path::new(&encoded_path).exists() {
        return Ok(SupervisorProjectConfig::new());
    }
    let bytes = std::fs::read(&encoded_path).map_err(|error| node_read_error(&error, path))?;
    let parsed =
        js_json::parse_lossless(String::from_utf8_lossy(&bytes).as_ref()).map_err(|error| error.into_message())?;
    let Some(object) = record(&parsed) else {
        return Err(js_format!("Supervisor project config must be an object: {}", path));
    };
    js_json::object_entries_lossless(object)
        .into_iter()
        .map(|(project_id, value)| {
            let Some(mapping) = record(value) else {
                return Err(js_format!(
                    "Supervisor project mapping must be an object: {}",
                    project_id
                ));
            };
            Ok((
                project_id.clone(),
                SupervisorProjectMapping {
                    remotes: read_string_array(mapping.get("remotes"))?,
                    roots: read_string_array(mapping.get("roots"))?,
                },
            ))
        })
        .collect()
}

/// `resolveSupervisorProject`. `path.resolve` reads `process.cwd()`, whose failure propagates.
pub fn resolve_supervisor_project(
    config: &SupervisorProjectConfig,
    cwd: &JsString,
    remote_repository_names: &[JsString],
) -> Result<JsString, JsString> {
    resolve_supervisor_project_with(&ResolveSupervisorProjectInput {
        config,
        cwd,
        remote_repository_names,
    })
}

fn resolve_supervisor_project_with(input: &ResolveSupervisorProjectInput<'_>) -> Result<JsString, JsString> {
    if let Some(project) = find_project_by_root(input.config, input.cwd)? {
        return Ok(project.clone());
    }
    if let Some(project) = find_project_by_remote(input.config, input.remote_repository_names) {
        return Ok(project.clone());
    }
    match input.remote_repository_names.first() {
        Some(name) => Ok(last_segment(name).to_lowercase()),
        None => Ok(node_basename(resolve(input.cwd)?)),
    }
}

fn resolve(path: &JsString) -> Result<JsString, JsString> {
    node_resolve("", path.clone()).map_err(|error| error.message().clone())
}

fn record(value: &JsValue) -> Option<&JsObject> {
    value.as_object()
}

fn read_string_array(value: Option<&JsValue>) -> Result<Option<Vec<JsString>>, JsString> {
    let Some(value) = value else {
        return Ok(None);
    };
    value
        .as_array()
        .and_then(|entries| entries.iter().map(|entry| entry.as_string().cloned()).collect())
        .map(Some)
        .ok_or_else(|| "Supervisor project mapping values must be string arrays".into())
}

fn find_project_by_root<'a>(
    config: &'a SupervisorProjectConfig,
    cwd: &JsString,
) -> Result<Option<&'a JsString>, JsString> {
    let resolved_cwd = resolve(cwd)?;
    for (project_id, mapping) in config.iter() {
        for root in mapping.roots.iter().flatten() {
            if is_path_within_root(&resolved_cwd, &resolve(root)?) {
                return Ok(Some(project_id));
            }
        }
    }
    Ok(None)
}

fn find_project_by_remote<'a>(
    config: &'a SupervisorProjectConfig,
    remote_repository_names: &[JsString],
) -> Option<&'a JsString> {
    let remote_names: HashSet<JsString> = remote_repository_names.iter().map(JsString::to_lowercase).collect();
    config.iter().find_map(|(project_id, mapping)| {
        mapping
            .remotes
            .as_ref()?
            .iter()
            .any(|remote| remote_names.contains(&remote.to_lowercase()))
            .then_some(project_id)
    })
}

/// `readRemoteRepositoryNames`: `spawnSync` reports a spawn failure as a non-zero status.
pub fn read_remote_repository_names(cwd: &JsString) -> Vec<JsString> {
    let Ok(output) = spawn_sync(
        &"git".into(),
        &["remote".into(), "-v".into()],
        SpawnSyncOptions {
            cwd: Some(Path::new(&encode_node_utf8(cwd))),
            ..Default::default()
        },
    ) else {
        return Vec::new();
    };
    if output.error.is_some() || output.status != Some(0) {
        return Vec::new();
    }
    let stdout =
        JsString::from(String::from_utf8_lossy(&output.stdout.expect("successful sync capture has stdout")).as_ref());
    let mut names: Vec<JsString> = Vec::new();
    for line in stdout.split('\n') {
        let Some(remote_url) = second_whitespace_field(&line.trim()) else {
            continue;
        };
        for name in [
            extract_remote_repository_identity(&remote_url),
            extract_remote_repository_name(&remote_url),
        ]
        .into_iter()
        .flatten()
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// `line.split(/\s+/)[1]` on an already-trimmed line, filtered by `Boolean`.
fn second_whitespace_field(line: &JsString) -> Option<JsString> {
    let units = line.units();
    let first_end = units.iter().position(|unit| is_js_whitespace_unit(*unit))?;
    let start = first_end
        + units[first_end..]
            .iter()
            .take_while(|unit| is_js_whitespace_unit(**unit))
            .count();
    let length = units[start..]
        .iter()
        .take_while(|unit| !is_js_whitespace_unit(**unit))
        .count();
    (length > 0).then(|| line.slice(start, start + length))
}

fn is_path_within_root(path: &JsString, root: &JsString) -> bool {
    path == root || path.starts_with(root.clone() + "/")
}

fn last_segment(value: &JsString) -> JsString {
    let start = value
        .units()
        .iter()
        .rposition(|unit| *unit == SLASH)
        .map_or(0, |index| index + 1);
    value.slice(start, value.len())
}
