//! Port of `packages/coding-agent/src/core/trust-manager.ts`.
//!
//! `trust.json` is a JSON object keyed by canonical project path. Reads and writes happen under the
//! same `proper-lockfile` protocol as the TypeScript runtime (`<path>.lock`), so both runtimes can
//! share an agent directory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};

use crate::config::package_config;
use crate::utils::lockfile::{LockOptions, acquire_sync};
use crate::utils::paths::{
    PathError, PathInputOptions, canonicalize_path, home_directory, node_dirname, node_join, process_cwd, resolve_path,
};

const TRUST_REQUIRING_PROJECT_CONFIG_RESOURCES: [&str; 8] = [
    "settings.json",
    "extensions",
    "skills",
    "prompts",
    "themes",
    "rules",
    "SYSTEM.md",
    "APPEND_SYSTEM.md",
];

/// `ProjectTrustDecision`: `null` inherits from a parent directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTrustDecision {
    Unknown,
    Trusted,
    Untrusted,
}

impl ProjectTrustDecision {
    fn from_value(value: &JsValue) -> Option<Self> {
        match value {
            JsValue::Bool(true) => Some(Self::Trusted),
            JsValue::Bool(false) => Some(Self::Untrusted),
            JsValue::Null => Some(Self::Unknown),
            _ => None,
        }
    }

    fn to_value(self) -> JsValue {
        match self {
            Self::Unknown => JsValue::Null,
            Self::Trusted => JsValue::Bool(true),
            Self::Untrusted => JsValue::Bool(false),
        }
    }
}

/// A trust entry that applies to a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustStoreEntry {
    pub path: JsString,
    pub decision: ProjectTrustDecision,
}

/// One path decision to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustUpdate {
    pub path: JsString,
    pub decision: ProjectTrustDecision,
}

/// A trust choice offered to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustOption {
    pub label: JsString,
    pub trusted: bool,
    pub updates: Vec<ProjectTrustUpdate>,
    pub saved_path: Option<JsString>,
}

/// Failure reading or writing the trust store.
#[derive(Debug, Clone, PartialEq)]
pub struct TrustError {
    message: JsString,
    callback: Option<Box<pi_ai::request::CallbackError>>,
}

impl TrustError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            callback: None,
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl From<PathError> for TrustError {
    fn from(error: PathError) -> Self {
        Self::new(error.message().clone())
    }
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl From<pi_ai::request::CallbackError> for TrustError {
    fn from(error: pi_ai::request::CallbackError) -> Self {
        Self {
            message: error.message.clone(),
            callback: Some(Box::new(error)),
        }
    }
}

impl std::error::Error for TrustError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.callback.as_deref().map(|error| error as &dyn std::error::Error)
    }
}

/// `resolvePath(path)` against `process.cwd()`.
fn resolve_from_process_cwd(path: &JsString) -> Result<JsString, PathError> {
    resolve_path(path, process_cwd()?, &PathInputOptions::default())
}

fn normalize_cwd(cwd: &JsString) -> Result<JsString, PathError> {
    Ok(canonicalize_path(resolve_from_process_cwd(cwd)?))
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

fn find_nearest_trust_entry(data: &JsObject, cwd: &JsString) -> Result<Option<ProjectTrustStoreEntry>, PathError> {
    let mut current_dir = normalize_cwd(cwd)?;
    loop {
        let decision = data.get(&current_dir).and_then(ProjectTrustDecision::from_value);
        if let Some(decision @ (ProjectTrustDecision::Trusted | ProjectTrustDecision::Untrusted)) = decision {
            return Ok(Some(ProjectTrustStoreEntry {
                path: current_dir,
                decision,
            }));
        }
        let parent_dir = node_dirname(&current_dir);
        if parent_dir == current_dir {
            return Ok(None);
        }
        current_dir = parent_dir;
    }
}

/// The parent directory of the trust path, or `None` at the filesystem root.
pub fn get_project_trust_parent_path(cwd: &JsString) -> Result<Option<JsString>, PathError> {
    let trust_path = normalize_cwd(cwd)?;
    let parent = node_dirname(&trust_path);
    Ok((parent != trust_path).then_some(parent))
}

/// `getProjectTrustOptions`: the choices for a project directory.
pub fn get_project_trust_options(
    cwd: &JsString,
    include_session_only: bool,
) -> Result<Vec<ProjectTrustOption>, PathError> {
    let trust_path = normalize_cwd(cwd)?;
    let mut options = vec![ProjectTrustOption {
        label: "Trust".into(),
        trusted: true,
        updates: vec![ProjectTrustUpdate {
            path: trust_path.clone(),
            decision: ProjectTrustDecision::Trusted,
        }],
        saved_path: Some(trust_path.clone()),
    }];
    if let Some(parent_path) = get_project_trust_parent_path(cwd)? {
        options.push(ProjectTrustOption {
            label: js_format!("Trust parent folder ({})", parent_path),
            trusted: true,
            updates: vec![
                ProjectTrustUpdate {
                    path: parent_path.clone(),
                    decision: ProjectTrustDecision::Trusted,
                },
                ProjectTrustUpdate {
                    path: trust_path.clone(),
                    decision: ProjectTrustDecision::Unknown,
                },
            ],
            saved_path: Some(parent_path),
        });
    }
    if include_session_only {
        options.push(ProjectTrustOption {
            label: "Trust (this session only)".into(),
            trusted: true,
            updates: Vec::new(),
            saved_path: None,
        });
    }
    options.push(ProjectTrustOption {
        label: "Do not trust".into(),
        trusted: false,
        updates: vec![ProjectTrustUpdate {
            path: trust_path.clone(),
            decision: ProjectTrustDecision::Untrusted,
        }],
        saved_path: Some(trust_path),
    });
    if include_session_only {
        options.push(ProjectTrustOption {
            label: "Do not trust (this session only)".into(),
            trusted: false,
            updates: Vec::new(),
            saved_path: None,
        });
    }
    Ok(options)
}

fn read_trust_file(path: &JsString) -> Result<JsObject, TrustError> {
    if !path_exists(path) {
        return Ok(JsObject::new());
    }
    let encoded_path = encode_node_utf8(path);
    let read_error =
        |message: JsString| TrustError::new(js_format!("Failed to read trust store {}: {}", path, message));
    let bytes = std::fs::read(&encoded_path)
        .map_err(|error| read_error(pi_js_compat::node_fs::node_read_error(&error, path)))?;
    // `readFileSync(path, "utf-8")` decodes with replacement.
    let content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
    let parsed = js_json::parse_lossless(content).map_err(|error| read_error(error.message().clone()))?;
    let JsValue::Object(object) = parsed else {
        return Err(TrustError::new(js_format!(
            "Invalid trust store {}: expected an object",
            path
        )));
    };
    for (key, value) in js_json::object_entries_lossless(&object) {
        if ProjectTrustDecision::from_value(value).is_none() {
            return Err(TrustError::new(js_format!(
                "Invalid trust store {}: value for {} must be true, false, or null",
                path,
                js_json::stringify_lossless(&JsValue::String(key.clone()))
            )));
        }
    }
    Ok(object)
}

fn write_trust_file(path: &JsString, data: &JsObject) -> Result<(), TrustError> {
    let write_error = |error: std::io::Error| {
        TrustError::new(js_format!(
            "Failed to write trust store {}: {}",
            path,
            error.to_string()
        ))
    };
    let mut keys: Vec<&JsString> = data.keys().collect();
    // `Object.keys(data).sort()` orders by UTF-16 code units.
    keys.sort_by(|left, right| left.units().cmp(right.units()));
    let sorted: JsObject = keys
        .into_iter()
        .filter_map(|key| {
            let value = data.get(key)?;
            ProjectTrustDecision::from_value(value).map(|_| (key.clone(), value.clone()))
        })
        .collect();
    std::fs::create_dir_all(encode_node_utf8(&node_dirname(path))).map_err(write_error)?;
    std::fs::write(
        encode_node_utf8(path),
        format!("{}\n", js_json::stringify_lossless_pretty(&JsValue::Object(sorted))),
    )
    .map_err(write_error)
}

/// Acquires `<path>.lock` with ten attempts twenty milliseconds apart, like the TypeScript store.
fn acquire_trust_lock(path: &JsString) -> Result<crate::utils::lockfile::LockGuard, TrustError> {
    let encoded_path = encode_node_utf8(path);
    let directory = encode_node_utf8(&node_dirname(path));
    std::fs::create_dir_all(&directory).map_err(|error| {
        TrustError::new(js_format!(
            "Failed to create trust directory {}: {}",
            node_dirname(path),
            error.to_string()
        ))
    })?;
    let options = LockOptions {
        lockfile_path: Some(PathBuf::from(format!("{encoded_path}.lock"))),
        realpath: false,
        ..LockOptions::default()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_error = None;
    for _ in 0..10 {
        match acquire_sync(Path::new(&encoded_path), &options) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.code == crate::utils::lockfile::LockErrorCode::Locked => {
                last_error = Some(error);
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(TrustError::new(error.to_string())),
        }
    }
    Err(TrustError::new(last_error.map_or_else(
        || "Failed to acquire trust store lock".to_owned(),
        |error| error.to_string(),
    )))
}

/// JSON-backed project trust decisions (`<agentDir>/trust.json`).
pub struct ProjectTrustStore {
    trust_path: JsString,
}

impl ProjectTrustStore {
    pub fn new(agent_dir: &JsString) -> Result<Self, PathError> {
        Ok(Self {
            trust_path: node_join(resolve_from_process_cwd(agent_dir)?, "trust.json"),
        })
    }

    pub fn trust_path(&self) -> &JsString {
        &self.trust_path
    }

    pub fn get(&self, cwd: &JsString) -> Result<ProjectTrustDecision, TrustError> {
        Ok(self
            .get_entry(cwd)?
            .map_or(ProjectTrustDecision::Unknown, |entry| entry.decision))
    }

    pub fn get_entry(&self, cwd: &JsString) -> Result<Option<ProjectTrustStoreEntry>, TrustError> {
        let _guard = acquire_trust_lock(&self.trust_path)?;
        let data = read_trust_file(&self.trust_path)?;
        Ok(find_nearest_trust_entry(&data, cwd)?)
    }

    pub fn set(&self, cwd: &JsString, decision: ProjectTrustDecision) -> Result<(), TrustError> {
        self.set_many(&[ProjectTrustUpdate {
            path: cwd.clone(),
            decision,
        }])
    }

    pub fn set_many(&self, decisions: &[ProjectTrustUpdate]) -> Result<(), TrustError> {
        let _guard = acquire_trust_lock(&self.trust_path)?;
        let mut data = read_trust_file(&self.trust_path)?;
        for update in decisions {
            let key = normalize_cwd(&update.path)?;
            if update.decision == ProjectTrustDecision::Unknown {
                data.shift_remove(&key);
            } else {
                data.insert(key, update.decision.to_value());
            }
        }
        write_trust_file(&self.trust_path, &data)
    }
}

/// `hasTrustRequiringProjectResources`: whether `cwd` (or an ancestor) holds resources that must be
/// gated by project trust. The user-level `~/.agents/skills`, `~/.codex/skills`, and
/// `~/.claude/skills` directories are always trusted and ignored.
pub fn has_trust_requiring_project_resources(cwd: &JsString) -> Result<bool, PathError> {
    let home_dir = canonicalize_path(resolve_from_process_cwd(&home_directory()?)?);
    let user_agents_skills_dir = node_join(node_join(&home_dir, ".agents"), "skills");
    let user_tool_skill_dirs = [
        node_join(node_join(&home_dir, ".codex"), "skills"),
        node_join(node_join(&home_dir, ".claude"), "skills"),
    ];
    let mut current_dir = canonicalize_path(resolve_from_process_cwd(cwd)?);

    let config_dir = node_join(&current_dir, &package_config().config_dir_name);
    if TRUST_REQUIRING_PROJECT_CONFIG_RESOURCES
        .iter()
        .any(|entry| path_exists(&node_join(&config_dir, *entry)))
    {
        return Ok(true);
    }

    loop {
        let agents_skills_dir = node_join(node_join(&current_dir, ".agents"), "skills");
        if agents_skills_dir != user_agents_skills_dir && path_exists(&agents_skills_dir) {
            return Ok(true);
        }
        let codex_skills_dir = node_join(node_join(&current_dir, ".codex"), "skills");
        let claude_skills_dir = node_join(node_join(&current_dir, ".claude"), "skills");
        let has_project_codex_skills =
            !user_tool_skill_dirs.contains(&codex_skills_dir) && path_exists(&codex_skills_dir);
        let has_project_claude_skills =
            !user_tool_skill_dirs.contains(&claude_skills_dir) && path_exists(&claude_skills_dir);
        if has_project_codex_skills || has_project_claude_skills {
            return Ok(true);
        }

        let parent_dir = node_dirname(&current_dir);
        if parent_dir == current_dir {
            return Ok(false);
        }
        current_dir = parent_dir;
    }
}
