//! Supervisor project resolution (`supervisor/project-resolver.ts`).

use std::path::Path;

use serde::ser::SerializeMap;
use serde_json::Value;

use crate::utils::child_process::spawn_process_sync;
use crate::utils::paths::lexical_normalize;

pub const DEFAULT_SUPERVISOR_KB_DIR: &str = "/syncthing/Sync/KB";

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SupervisorProjectMapping {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remotes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<Vec<String>>,
}

/// Project mappings in JavaScript `Object.entries` order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupervisorProjectConfig(Vec<(String, SupervisorProjectMapping)>);

impl SupervisorProjectConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, project_id: impl Into<String>, mapping: SupervisorProjectMapping) {
        self.0.push((project_id.into(), mapping));
    }

    pub fn get(&self, project_id: &str) -> Option<&SupervisorProjectMapping> {
        self.0
            .iter()
            .find(|(existing, _)| existing == project_id)
            .map(|(_, mapping)| mapping)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &SupervisorProjectMapping)> {
        self.0
            .iter()
            .map(|(project_id, mapping)| (project_id.as_str(), mapping))
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

impl FromIterator<(String, SupervisorProjectMapping)> for SupervisorProjectConfig {
    fn from_iter<T: IntoIterator<Item = (String, SupervisorProjectMapping)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

pub struct ResolveSupervisorProjectInput<'a> {
    pub config: &'a SupervisorProjectConfig,
    pub cwd: &'a str,
    pub remote_repository_names: &'a [String],
}

pub fn extract_remote_repository_identity(remote_url: &str) -> Option<String> {
    let normalized = strip_authority(remote_url.trim());
    let segments: Vec<&str> = normalized.split('/').filter(|segment| !segment.is_empty()).collect();
    match segments.len() {
        0 => None,
        1 => Some(segments[0].to_owned()),
        length => Some(format!("{}/{}", segments[length - 2], segments[length - 1])),
    }
}

pub fn extract_remote_repository_name(remote_url: &str) -> Option<String> {
    extract_remote_repository_identity(remote_url).and_then(|identity| identity.rsplit('/').next().map(str::to_owned))
}

fn strip_authority(remote_url: &str) -> String {
    let trimmed = remote_url.trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let without_scheme = strip_scheme_authority(without_git);
    strip_scp_authority(without_scheme).to_owned()
}

fn strip_scheme_authority(value: &str) -> &str {
    let Some(scheme_end) = value.find("://") else {
        return value;
    };
    let scheme = &value[..scheme_end];
    let is_scheme = scheme.chars().next().is_some_and(|first| first.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-'));
    if !is_scheme {
        return value;
    }
    let remainder = &value[scheme_end + 3..];
    match remainder.find('/') {
        Some(authority_end) => &remainder[authority_end + 1..],
        None => "",
    }
}

fn strip_scp_authority(value: &str) -> &str {
    let Some(colon) = value.find(':') else {
        return value;
    };
    let prefix = &value[..colon];
    let has_user = prefix.contains('@');
    let has_slash = prefix.contains('/');
    if has_user && !has_slash {
        &value[colon + 1..]
    } else {
        value
    }
}

pub fn resolve_supervisor_project(
    config: &SupervisorProjectConfig,
    cwd: &str,
    remote_repository_names: &[String],
) -> String {
    resolve_supervisor_project_with(&ResolveSupervisorProjectInput {
        config,
        cwd,
        remote_repository_names,
    })
}

fn resolve_supervisor_project_with(input: &ResolveSupervisorProjectInput<'_>) -> String {
    let resolved_cwd = resolve_like_node(input.cwd);
    if let Some(project) = find_project_by_root(input.config, &resolved_cwd) {
        return project.to_owned();
    }
    if let Some(project) = find_project_by_remote(input.config, input.remote_repository_names) {
        return project.to_owned();
    }
    input
        .remote_repository_names
        .first()
        .map(|name| last_segment(name).to_lowercase())
        .unwrap_or_else(|| basename(&resolved_cwd))
}

pub fn resolve_supervisor_project_for_cwd(cwd: &str, kb_dir: &str) -> Result<String, String> {
    let config_path = Path::new(kb_dir)
        .join("memory")
        .join("supervisor")
        .join("projects.json");
    let config = read_supervisor_project_config(&config_path)?;
    Ok(resolve_supervisor_project(
        &config,
        cwd,
        &read_remote_repository_names(cwd),
    ))
}

/// The error message is the source text; JSON syntax errors come from `serde_json`.
pub fn read_supervisor_project_config(path: &Path) -> Result<SupervisorProjectConfig, String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(SupervisorProjectConfig::new());
    };
    let text = String::from_utf8_lossy(&bytes);
    let parsed: Value = pi_js_compat::js_json::parse(&text).map_err(|error| error.to_string())?;
    let Some(object) = parsed.as_object() else {
        return Err(format!(
            "Supervisor project config must be an object: {}",
            path.display()
        ));
    };
    let entries: Vec<(&String, &Value)> = object.iter().collect();
    let order = javascript_object_order(entries.iter().map(|(project_id, _)| project_id.as_str()));
    let mut config = SupervisorProjectConfig::new();
    for (project_id, value) in order.into_iter().map(|index| entries[index]) {
        let Some(mapping) = value.as_object() else {
            return Err(format!("Supervisor project mapping must be an object: {project_id}"));
        };
        config.insert(
            project_id.clone(),
            SupervisorProjectMapping {
                remotes: read_string_array(mapping.get("remotes"))?,
                roots: read_string_array(mapping.get("roots"))?,
            },
        );
    }
    Ok(config)
}

/// `Object.keys` puts canonical array-index keys first, ascending, then insertion order.
fn javascript_object_order(keys: impl Iterator<Item = impl AsRef<str>>) -> Vec<usize> {
    let mut indices: Vec<(Option<u32>, usize)> = keys
        .enumerate()
        .map(|(index, key)| (array_index(key.as_ref()), index))
        .collect();
    indices.sort_by_key(|(array_index, index)| match array_index {
        Some(value) => (*value, *index),
        None => (u32::MAX, *index),
    });
    indices.into_iter().map(|(_, index)| index).collect()
}

fn array_index(key: &str) -> Option<u32> {
    let canonical = key == "0" || (!key.starts_with('0') && !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()));
    canonical
        .then(|| key.parse::<u32>().ok())
        .flatten()
        .filter(|value| *value < u32::MAX)
}

fn read_string_array(value: Option<&Value>) -> Result<Option<Vec<String>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(entries) = value.as_array() else {
        return Err("Supervisor project mapping values must be string arrays".to_owned());
    };
    entries
        .iter()
        .map(|entry| entry.as_str().map(str::to_owned))
        .collect::<Option<Vec<String>>>()
        .map(Some)
        .ok_or_else(|| "Supervisor project mapping values must be string arrays".to_owned())
}

fn find_project_by_root<'a>(config: &'a SupervisorProjectConfig, resolved_cwd: &str) -> Option<&'a str> {
    config.iter().find_map(|(project_id, mapping)| {
        mapping
            .roots
            .as_ref()?
            .iter()
            .any(|root| is_path_within_root(resolved_cwd, &resolve_like_node(root)))
            .then_some(project_id)
    })
}

fn find_project_by_remote<'a>(
    config: &'a SupervisorProjectConfig,
    remote_repository_names: &[String],
) -> Option<&'a str> {
    let remote_names: Vec<String> = remote_repository_names.iter().map(|name| name.to_lowercase()).collect();
    config.iter().find_map(|(project_id, mapping)| {
        mapping
            .remotes
            .as_ref()?
            .iter()
            .any(|remote| remote_names.contains(&remote.to_lowercase()))
            .then_some(project_id)
    })
}

pub fn read_remote_repository_names(cwd: &str) -> Vec<String> {
    let Ok(output) = spawn_process_sync("git", &["remote", "-v"], Some(Path::new(cwd))) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut names: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let Some(remote_url) = line.split_whitespace().nth(1) else {
            continue;
        };
        for name in [
            extract_remote_repository_identity(remote_url),
            extract_remote_repository_name(remote_url),
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

fn is_path_within_root(path: &str, root: &str) -> bool {
    path == root || path.starts_with(&format!("{root}{}", std::path::MAIN_SEPARATOR))
}

/// `path.resolve`: lexical normalization, relative paths resolved against the process cwd.
fn resolve_like_node(path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        return lexical_normalize(path);
    }
    let current = std::env::current_dir().expect("process cwd must be readable");
    lexical_normalize(&current.join(path))
}

fn basename(resolved: &str) -> String {
    Path::new(resolved)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn last_segment(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}
