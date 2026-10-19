//! Port of `packages/coding-agent/src/core/trust-manager.ts`.
//!
//! `trust.json` is a JSON object keyed by canonical project path. Reads and writes happen under the
//! same `proper-lockfile` protocol as the TypeScript runtime (`<path>.lock`), so both runtimes can
//! share an agent directory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::package_config;
use crate::utils::js_json;
use crate::utils::lockfile::{LockOptions, acquire_sync};
use crate::utils::paths::{PathInputOptions, canonicalize_path, resolve_path};

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
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Bool(true) => Some(Self::Trusted),
            serde_json::Value::Bool(false) => Some(Self::Untrusted),
            serde_json::Value::Null => Some(Self::Unknown),
            _ => None,
        }
    }

    fn to_value(self) -> serde_json::Value {
        match self {
            Self::Unknown => serde_json::Value::Null,
            Self::Trusted => serde_json::Value::Bool(true),
            Self::Untrusted => serde_json::Value::Bool(false),
        }
    }

    fn decided(self) -> Option<bool> {
        match self {
            Self::Unknown => None,
            Self::Trusted => Some(true),
            Self::Untrusted => Some(false),
        }
    }
}

/// A trust entry that applies to a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustStoreEntry {
    pub path: String,
    pub decision: ProjectTrustDecision,
}

/// One path decision to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustUpdate {
    pub path: String,
    pub decision: ProjectTrustDecision,
}

/// A trust choice offered to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTrustOption {
    pub label: String,
    pub trusted: bool,
    pub updates: Vec<ProjectTrustUpdate>,
    pub saved_path: Option<String>,
}

/// Failure reading or writing the trust store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustError {
    message: String,
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TrustError {}

fn path_options() -> PathInputOptions {
    PathInputOptions::default()
}

fn current_dir() -> String {
    std::env::current_dir()
        .map(|directory| directory.display().to_string())
        .unwrap_or_else(|_| ".".to_owned())
}

fn normalize_cwd(cwd: &str) -> String {
    let resolved = resolve_path(cwd, &current_dir(), &path_options()).unwrap_or_else(|_| cwd.to_owned());
    canonicalize_path(&resolved)
}

fn dirname(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => "/".to_owned(),
        Some(index) => trimmed[..index].to_owned(),
        None => ".".to_owned(),
    }
}

fn join(base: &str, segment: &str) -> String {
    PathBuf::from(base).join(segment).display().to_string()
}

/// The parent directory of the trust path, or `None` at the filesystem root.
pub fn get_project_trust_parent_path(cwd: &str) -> Option<String> {
    let trust_path = normalize_cwd(cwd);
    let parent = dirname(&trust_path);
    (parent != trust_path).then_some(parent)
}

/// `getProjectTrustOptions`: the choices for a project directory.
pub fn get_project_trust_options(cwd: &str, include_session_only: bool) -> Vec<ProjectTrustOption> {
    let trust_path = normalize_cwd(cwd);
    let mut options = vec![ProjectTrustOption {
        label: "Trust".to_owned(),
        trusted: true,
        updates: vec![ProjectTrustUpdate {
            path: trust_path.clone(),
            decision: ProjectTrustDecision::Trusted,
        }],
        saved_path: Some(trust_path.clone()),
    }];
    if let Some(parent_path) = get_project_trust_parent_path(cwd) {
        options.push(ProjectTrustOption {
            label: format!("Trust parent folder ({parent_path})"),
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
            label: "Trust (this session only)".to_owned(),
            trusted: true,
            updates: Vec::new(),
            saved_path: None,
        });
    }
    options.push(ProjectTrustOption {
        label: "Do not trust".to_owned(),
        trusted: false,
        updates: vec![ProjectTrustUpdate {
            path: trust_path.clone(),
            decision: ProjectTrustDecision::Untrusted,
        }],
        saved_path: Some(trust_path),
    });
    if include_session_only {
        options.push(ProjectTrustOption {
            label: "Do not trust (this session only)".to_owned(),
            trusted: false,
            updates: Vec::new(),
            saved_path: None,
        });
    }
    options
}

type TrustFile = Vec<(String, serde_json::Value)>;

fn read_trust_file(path: &str) -> Result<TrustFile, TrustError> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path).map_err(|error| TrustError {
        message: format!("Failed to read trust store {path}: {error}"),
    })?;
    let parsed = js_json::parse(&content).map_err(|error| TrustError {
        message: format!("Failed to read trust store {path}: {error}"),
    })?;
    let serde_json::Value::Object(object) = parsed else {
        return Err(TrustError {
            message: format!("Invalid trust store {path}: expected an object"),
        });
    };
    let mut data = Vec::new();
    for (key, value) in object {
        if ProjectTrustDecision::from_value(&value).is_none() {
            return Err(TrustError {
                message: format!(
                    "Invalid trust store {path}: value for {} must be true, false, or null",
                    serde_json::Value::String(key.clone())
                ),
            });
        }
        data.push((key, value));
    }
    Ok(data)
}

fn write_trust_file(path: &str, data: &TrustFile) -> Result<(), TrustError> {
    let mut sorted: Vec<&(String, serde_json::Value)> = data.iter().collect();
    // `Object.keys(data).sort()` orders by UTF-16 code units.
    sorted.sort_by_key(|(key, _)| key.encode_utf16().collect::<Vec<_>>());
    let object: serde_json::Map<String, serde_json::Value> = sorted
        .into_iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|error| TrustError {
            message: format!("Failed to write trust store {path}: {error}"),
        })?;
    }
    std::fs::write(
        path,
        format!("{}\n", js_json::stringify_pretty(&serde_json::Value::Object(object))),
    )
    .map_err(|error| TrustError {
        message: format!("Failed to write trust store {path}: {error}"),
    })
}

/// Acquires `<path>.lock` with ten attempts twenty milliseconds apart, like the TypeScript store.
fn acquire_trust_lock(path: &str) -> Result<crate::utils::lockfile::LockGuard, TrustError> {
    if let Some(directory) = Path::new(path).parent() {
        std::fs::create_dir_all(directory).map_err(|error| TrustError {
            message: format!("Failed to create trust directory {}: {error}", directory.display()),
        })?;
    }
    let options = LockOptions {
        lockfile_path: Some(PathBuf::from(format!("{path}.lock"))),
        realpath: false,
        ..LockOptions::default()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_error = None;
    for _ in 0..10 {
        match acquire_sync(Path::new(path), &options) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.code == crate::utils::lockfile::LockErrorCode::Locked => {
                last_error = Some(error);
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(TrustError {
                    message: error.to_string(),
                });
            }
        }
    }
    Err(TrustError {
        message: last_error.map_or_else(
            || "Failed to acquire trust store lock".to_owned(),
            |error| error.to_string(),
        ),
    })
}

fn find_nearest_trust_entry(data: &TrustFile, cwd: &str) -> Option<ProjectTrustStoreEntry> {
    let mut current_dir = normalize_cwd(cwd);
    loop {
        if let Some((_, value)) = data.iter().find(|(path, _)| path == &current_dir)
            && let Some(decision) = ProjectTrustDecision::from_value(value)
            && decision.decided().is_some()
        {
            return Some(ProjectTrustStoreEntry {
                path: current_dir,
                decision,
            });
        }
        let parent_dir = dirname(&current_dir);
        if parent_dir == current_dir {
            return None;
        }
        current_dir = parent_dir;
    }
}

/// JSON-backed project trust decisions (`<agentDir>/trust.json`).
pub struct ProjectTrustStore {
    trust_path: String,
}

impl ProjectTrustStore {
    pub fn new(agent_dir: &str) -> Self {
        let resolved =
            resolve_path(agent_dir, &current_dir(), &path_options()).unwrap_or_else(|_| agent_dir.to_owned());
        Self {
            trust_path: join(&resolved, "trust.json"),
        }
    }

    pub fn trust_path(&self) -> &str {
        &self.trust_path
    }

    pub fn get(&self, cwd: &str) -> Result<ProjectTrustDecision, TrustError> {
        Ok(self
            .get_entry(cwd)?
            .map_or(ProjectTrustDecision::Unknown, |entry| entry.decision))
    }

    pub fn get_entry(&self, cwd: &str) -> Result<Option<ProjectTrustStoreEntry>, TrustError> {
        let _guard = acquire_trust_lock(&self.trust_path)?;
        let data = read_trust_file(&self.trust_path)?;
        Ok(find_nearest_trust_entry(&data, cwd))
    }

    pub fn set(&self, cwd: &str, decision: ProjectTrustDecision) -> Result<(), TrustError> {
        self.set_many(&[ProjectTrustUpdate {
            path: cwd.to_owned(),
            decision,
        }])
    }

    pub fn set_many(&self, decisions: &[ProjectTrustUpdate]) -> Result<(), TrustError> {
        let _guard = acquire_trust_lock(&self.trust_path)?;
        let mut data = read_trust_file(&self.trust_path)?;
        for update in decisions {
            let key = normalize_cwd(&update.path);
            data.retain(|(path, _)| path != &key);
            if update.decision != ProjectTrustDecision::Unknown {
                data.push((key, update.decision.to_value()));
            }
        }
        write_trust_file(&self.trust_path, &data)
    }
}

/// `hasTrustRequiringProjectResources`: whether `cwd` (or an ancestor) holds resources that must be
/// gated by project trust. The user-level `~/.agents/skills`, `~/.codex/skills`, and
/// `~/.claude/skills` directories are always trusted and ignored.
pub fn has_trust_requiring_project_resources(cwd: &str) -> bool {
    let home = pi_js_compat::process_env::get("HOME")
        .filter(|home| !home.is_empty())
        .unwrap_or_else(|| ".".to_owned());
    let home_dir = canonicalize_path(&resolve_path(&home, &current_dir(), &path_options()).unwrap_or(home));
    let user_agents_skills_dir = join(&home_dir, ".agents/skills");
    let user_tool_skill_dirs = [join(&home_dir, ".codex/skills"), join(&home_dir, ".claude/skills")];
    let mut current_dir =
        canonicalize_path(&resolve_path(cwd, &current_dir(), &path_options()).unwrap_or_else(|_| cwd.to_owned()));

    let config_dir = join(&current_dir, &package_config().config_dir_name);
    if TRUST_REQUIRING_PROJECT_CONFIG_RESOURCES
        .iter()
        .any(|entry| Path::new(&join(&config_dir, entry)).exists())
    {
        return true;
    }

    loop {
        let agents_skills_dir = join(&current_dir, ".agents/skills");
        if agents_skills_dir != user_agents_skills_dir && Path::new(&agents_skills_dir).exists() {
            return true;
        }
        let codex_skills_dir = join(&current_dir, ".codex/skills");
        let claude_skills_dir = join(&current_dir, ".claude/skills");
        let has_project_codex_skills =
            !user_tool_skill_dirs.contains(&codex_skills_dir) && Path::new(&codex_skills_dir).exists();
        let has_project_claude_skills =
            !user_tool_skill_dirs.contains(&claude_skills_dir) && Path::new(&claude_skills_dir).exists();
        if has_project_codex_skills || has_project_claude_skills {
            return true;
        }

        let parent_dir = dirname(&current_dir);
        if parent_dir == current_dir {
            return false;
        }
        current_dir = parent_dir;
    }
}
