//! Cross-process account selection and dated subscription quota evidence.
use indexmap::IndexMap;
use pi_js_compat::callback_error::CallbackError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ACCOUNTS_EXHAUSTED: &str = "Claude account profiles exhausted. No eligible account remains.";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub rejected: IndexMap<String, f64>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub observations: IndexMap<String, Observation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub at: f64,
    pub available: bool,
    #[serde(rename = "recoveredAt", skip_serializing_if = "Option::is_none")]
    pub recovered_at: Option<f64>,
}

impl Observation {
    fn recovery(&self) -> f64 {
        if self.available {
            self.at
        } else {
            self.recovered_at.unwrap_or(f64::NEG_INFINITY)
        }
    }
}
fn missing_observation() -> Observation {
    Observation {
        at: 0.0,
        available: false,
        recovered_at: None,
    }
}
fn denial_reset(old: &Observation, new: &Observation, old_reset: Option<f64>, new_reset: Option<f64>) -> Option<f64> {
    let recovery = old.recovery().max(new.recovery());
    let old_reset = old_reset.filter(|_| !old.available && old.at >= recovery);
    let new_reset = new_reset.filter(|_| !new.available && new.at >= recovery);
    match (old_reset, new_reset) {
        (Some(a), Some(_)) if old.recovery() > new.recovery() => Some(a),
        (Some(_), Some(b)) if new.recovery() > old.recovery() => Some(b),
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
pub fn merge_states(mut previous: AccountState, incoming: AccountState) -> AccountState {
    let names: indexmap::IndexSet<_> = incoming.rejected.keys().chain(incoming.observations.keys()).collect();
    for name in names {
        let old = previous
            .observations
            .get(name)
            .cloned()
            .unwrap_or_else(missing_observation);
        let new = incoming
            .observations
            .get(name)
            .cloned()
            .unwrap_or_else(missing_observation);
        let old_wins = old.at > new.at || (old.at == new.at && !old.available);
        let mut latest = if old_wins { old.clone() } else { new.clone() };
        let recovered_at = old.recovery().max(new.recovery());
        latest.recovered_at = (!latest.available && recovered_at.is_finite()).then_some(recovered_at);
        let reset = denial_reset(
            &old,
            &new,
            previous.rejected.get(name).copied(),
            incoming.rejected.get(name).copied(),
        );
        if let Some(reset) = reset {
            previous.rejected.insert(name.clone(), reset);
        } else {
            previous.rejected.shift_remove(name);
        }
        previous.observations.insert(name.clone(), latest);
    }
    previous.current = incoming.current;
    previous
}

#[derive(Serialize, Deserialize)]
struct ProcessState {
    #[serde(flatten)]
    state: AccountState,
    #[serde(rename = "updatedAt")]
    updated_at: f64,
}
type FileVersion = (u64, std::time::SystemTime);
type MergedFiles = Vec<(PathBuf, FileVersion)>;
fn state_dir(home: &Path) -> PathBuf {
    home.join(".config/pi-claude-bridge/account-state")
}
fn version(path: &Path) -> std::io::Result<Option<FileVersion>> {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(stat) => Ok(Some((stat.ino(), stat.modified()?))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn parse_process_state(bytes: &[u8], path: &Path) -> Result<ProcessState, CallbackError> {
    let mut process: ProcessState = serde_json::from_slice(bytes).map_err(|_| {
        CallbackError::new(format!(
            "Invalid Claude account state in {}; delete it to reset account selection",
            path.display()
        ))
    })?;
    for name in process.state.rejected.keys() {
        process.state.observations.entry(name.clone()).or_insert(Observation {
            at: process.updated_at / 1000.0,
            available: false,
            recovered_at: None,
        });
    }
    Ok(process)
}
fn read_merged_state(home: &Path) -> Result<(Option<AccountState>, MergedFiles), CallbackError> {
    let entries = match std::fs::read_dir(state_dir(home)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, Vec::new())),
        Err(e) => return Err(CallbackError::new(e.to_string())),
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| CallbackError::new(e.to_string()))?;
    paths.retain(|path| {
        path.file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".json"))
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    });
    paths.sort();
    let any = !paths.is_empty();
    let mut merged = Vec::new();
    let mut combined = AccountState::default();
    let mut newest: Option<(f64, String)> = None;
    for path in paths {
        let file_version = version(&path).map_err(|e| CallbackError::new(e.to_string()))?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(CallbackError::new(e.to_string())),
        };
        let process = parse_process_state(&bytes, &path)?;
        if let Some(current) = &process.state.current
            && newest.as_ref().is_none_or(|(at, _)| process.updated_at > *at)
        {
            newest = Some((process.updated_at, current.clone()));
        }
        combined = merge_states(combined, process.state);
        if let Some(v) = file_version {
            merged.push((path, v));
        }
    }
    combined.current = newest.map(|(_, name)| name);
    Ok((any.then_some(combined), merged))
}
pub fn read_state(home: &Path) -> Result<Option<AccountState>, CallbackError> {
    read_merged_state(home).map(|(state, _)| state)
}
fn prune_merged_files(files: MergedFiles) -> std::io::Result<()> {
    for (path, merged_version) in files {
        let pid = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<i32>().ok())
            .and_then(rustix::process::Pid::from_raw);
        let alive = pid.is_some_and(|pid| match rustix::process::test_kill_process(pid) {
            Ok(()) | Err(rustix::io::Errno::PERM) => true,
            Err(_) => false,
        });
        if !alive && version(&path)? == Some(merged_version) {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}
pub fn update_state(
    home: &Path,
    change: impl FnOnce(Option<AccountState>) -> AccountState,
) -> Result<(), CallbackError> {
    use std::os::unix::fs::PermissionsExt;
    let (state, merged) = read_merged_state(home)?;
    let process = ProcessState {
        state: change(state),
        updated_at: now_seconds()? * 1000.0,
    };
    let dir = state_dir(home);
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        for path in [home.join(".config/pi-claude-bridge"), dir.clone()] {
            std::fs::create_dir_all(&path)?;
            if !std::fs::symlink_metadata(&path)?.is_dir() {
                return Err("Account profile path must be a directory, not a symlink".into());
            }
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        }
        let temp = tempfile::Builder::new().prefix(".write-").tempdir_in(&dir)?;
        let file = temp.path().join("state.json");
        let content = format!("{}\n", serde_json::to_string(&process)?);
        std::fs::write(&file, content)?;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        std::fs::rename(file, dir.join(format!("{}.json", std::process::id())))?;
        prune_merged_files(merged)?;
        Ok(())
    };
    write().map_err(|e| CallbackError::new(e.to_string()))
}
pub fn now_seconds() -> Result<f64, CallbackError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .map_err(|e| CallbackError::new(e.to_string()))
}

pub struct AccountManager {
    home: PathBuf,
    names: Vec<String>,
    state: AccountState,
    blocked: IndexMap<String, f64>,
}
impl AccountManager {
    pub fn new(home: PathBuf, names: Vec<String>) -> Result<Self, CallbackError> {
        super::account_profiles::validate_names(&names.iter().map(|n| n.as_str().into()).collect::<Vec<_>>())?;
        let active = super::account_profiles::read_active_profile(&home, None)?;
        let current = active
            .filter(|name| names.contains(name))
            .or_else(|| names.first().cloned());
        Ok(Self {
            home,
            names,
            state: AccountState {
                current,
                ..Default::default()
            },
            blocked: IndexMap::new(),
        })
    }
    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn selected_profile(&mut self) -> Result<Option<String>, CallbackError> {
        if let Some(shared) = read_state(&self.home)? {
            self.load_shared(shared);
        }
        Ok(self.state.current.clone())
    }
    fn load_shared(&mut self, shared: AccountState) {
        let current = shared
            .current
            .clone()
            .filter(|name| self.names.contains(name))
            .or_else(|| self.state.current.clone());
        self.state = merge_states(self.state.clone(), shared);
        self.state.current = current;
        self.state.rejected.retain(|name, _| self.names.contains(name));
        self.state.observations.retain(|name, _| self.names.contains(name));
    }
    fn with_shared<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> Result<T, CallbackError> {
        let home = self.home.clone();
        let mut result = None;
        update_state(&home, |shared| {
            if let Some(shared) = shared {
                self.load_shared(shared);
            }
            result = Some(change(self));
            self.state.clone()
        })?;
        result.ok_or_else(|| CallbackError::new("Account state update did not run"))
    }
    fn select_loaded(&mut self, now: f64) -> Option<String> {
        self.state.rejected.retain(|_, reset| *reset > now);
        let available = |name: &str| !self.state.rejected.contains_key(name) && !self.blocked.contains_key(name);
        if let Some(current) = &self.state.current
            && available(current)
        {
            return Some(current.clone());
        }
        let next = self.names.iter().find(|name| available(name)).cloned();
        if next.is_some() {
            self.state.current = next.clone();
        }
        next
    }
    pub fn select(&mut self, now: f64) -> Result<Option<String>, CallbackError> {
        self.with_shared(|manager| manager.select_loaded(now))
    }
    pub fn reject(&mut self, name: &str, reset: Option<f64>, now: f64) -> Result<Option<String>, CallbackError> {
        self.with_shared(|manager| {
            if let Some(reset) = reset.filter(|r| r.is_finite()) {
                let observation = manager.state.observations.get(name);
                if observation.is_none_or(|o| now >= o.at) {
                    let reset = manager
                        .state
                        .rejected
                        .get(name)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY)
                        .max(reset);
                    let recovered_at = observation.and_then(|o| if o.available { Some(o.at) } else { o.recovered_at });
                    manager.state.rejected.insert(name.into(), reset);
                    manager.state.observations.insert(
                        name.into(),
                        Observation {
                            at: now,
                            available: false,
                            recovered_at,
                        },
                    );
                }
            } else {
                let at = manager.blocked.get(name).copied().unwrap_or(f64::NEG_INFINITY).max(now);
                manager.blocked.insert(name.into(), at);
            }
            manager.select_loaded(now)
        })
    }
    pub fn recover(&mut self, name: &str, checked_at: f64) -> Result<bool, CallbackError> {
        if !self.names.iter().any(|n| n == name) || !checked_at.is_finite() {
            return Ok(false);
        }
        self.with_shared(|manager| {
            let observation = manager.state.observations.get(name);
            let denial = observation.filter(|o| !o.available).map_or(f64::NEG_INFINITY, |o| o.at);
            let local = manager.blocked.get(name).copied().unwrap_or(f64::NEG_INFINITY);
            if denial.max(local) >= checked_at {
                return false;
            }
            let at = observation.map_or(checked_at, |o| o.at.max(checked_at));
            manager.state.rejected.shift_remove(name);
            manager.blocked.shift_remove(name);
            manager.state.observations.insert(
                name.into(),
                Observation {
                    at,
                    available: true,
                    recovered_at: None,
                },
            );
            manager.state.current = Some(name.into());
            true
        })
    }
    pub fn prepare_selected(&mut self, now: f64) -> Result<(String, PathBuf), CallbackError> {
        let name = self
            .select(now)?
            .ok_or_else(|| CallbackError::new(ACCOUNTS_EXHAUSTED))?;
        let dir = super::account_profiles::prepare_profile(&self.home, &name)?;
        Ok((name, dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(text: &str) -> AccountState {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn claude_bridge_quota_evidence_uses_recovery_epochs_and_denial_ties() {
        let old = state(r#"{"current":"a","rejected":{"a":500},"observations":{"a":{"at":100,"available":false}}}"#);
        let recovered = state(r#"{"current":"b","rejected":{},"observations":{"a":{"at":110,"available":true}}}"#);
        let merged = merge_states(old.clone(), recovered.clone());
        assert!(merged.rejected.is_empty());
        assert_eq!(merged.current.as_deref(), Some("b"));
        let denied =
            state(r#"{"rejected":{"a":200},"observations":{"a":{"at":120,"available":false,"recoveredAt":110}}}"#);
        let expected = merge_states(recovered, denied);
        assert_eq!(merge_states(expected.clone(), old).rejected["a"], 200.0);
        let tied = state(r#"{"rejected":{},"observations":{"a":{"at":120,"available":true}}}"#);
        assert!(!merge_states(expected, tied).observations["a"].available);
    }

    #[test]
    fn claude_bridge_account_state_wire_round_trips_with_frozen_ts() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join("account-profiles.mjs");
        std::fs::write(&source, include_str!("fixtures/account-profiles.mjs")).unwrap();
        let original =
            state(r#"{"current":"a","rejected":{"a":500},"observations":{"a":{"at":100,"available":false}}}"#);
        update_state(home.path(), |_| original.clone()).unwrap();
        // Credential trace is an unused dependency in this state-only oracle.
        // Fail explicitly if the oracle ever reaches it instead of hiding a side effect.
        let script = r#"
const fs = require('node:fs'), vm = require('node:vm');
(async () => {
  const module = new vm.SourceTextModule(fs.readFileSync(process.argv[1], 'utf8'));
  await module.link(async specifier => {
    const values = specifier === './auth-trace.js'
      ? { recordCredentialTrace() { throw new Error('unexpected credential trace in state-only oracle'); } }
      : require(specifier);
    return new vm.SyntheticModule(Object.keys(values), function() {
      for (const [key, value] of Object.entries(values)) this.setExport(key, value);
    });
  });
  await module.evaluate();
  const home = process.argv[2];
  process.stdout.write(JSON.stringify(module.namespace.readAccountState(home)));
  module.namespace.updateAccountState(home, old => ({ ...old, current: 'b' }));
})().catch(error => { console.error(error); process.exitCode = 1; });
"#;
        let result = std::process::Command::new("node")
            .args(["--experimental-vm-modules", "-e", script])
            .arg(source)
            .arg(home.path())
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8(result.stderr).unwrap());
        let returned: AccountState = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(returned, original);
        let native = read_state(home.path()).unwrap().unwrap();
        assert_eq!(native.current.as_deref(), Some("b"));
        assert_eq!(native.rejected, original.rejected);
        assert_eq!(native.observations, original.observations);
    }

    #[test]
    fn claude_bridge_account_selection_shares_denials_but_not_undated_blocks() {
        let home = tempfile::tempdir().unwrap();
        let names = vec!["a".into(), "b".into()];
        let mut first = AccountManager::new(home.path().into(), names.clone()).unwrap();
        let mut second = AccountManager::new(home.path().into(), names.clone()).unwrap();
        assert_eq!(first.select(10.0).unwrap().as_deref(), Some("a"));
        assert_eq!(first.reject("a", Some(30.0), 11.0).unwrap().as_deref(), Some("b"));
        assert_eq!(second.select(12.0).unwrap().as_deref(), Some("b"));
        assert_eq!(second.reject("b", None, 13.0).unwrap(), None);
        assert!(!second.recover("b", 13.0).unwrap());
        assert!(second.recover("b", 14.0).unwrap());
        let mut restarted = AccountManager::new(home.path().into(), names).unwrap();
        assert_eq!(restarted.select(15.0).unwrap().as_deref(), Some("b"));
        assert!(!restarted.recover("a", 10.0).unwrap());
        assert!(restarted.recover("a", 31.0).unwrap());
        assert_eq!(first.select(32.0).unwrap().as_deref(), Some("a"));
        let bytes = std::fs::read(home.path().join(format!(
            ".config/pi-claude-bridge/account-state/{}.json",
            std::process::id()
        )))
        .unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(wire["updatedAt"].is_number());
        assert_eq!(wire["current"], "a");
    }
}
