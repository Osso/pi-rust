//! Frozen orchestrator path and package metadata configuration.
use std::{collections::BTreeMap, path::PathBuf};

pub const VERSION: &str = "0.80.3";
pub const IS_BUN_BINARY: bool = false;

pub fn orchestrator_dir(env: &BTreeMap<String, String>, home: &str) -> PathBuf {
    if let Some(dir) = env.get("PI_ORCHESTRATOR_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let pi_dir = env
        .get("PI_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".pi"));
    pi_dir.join("orchestrator")
}

#[derive(Debug, Clone)]
pub struct Config {
    pub dir: PathBuf,
}

impl Config {
    pub fn from_environment() -> Self {
        let env = std::env::vars().collect();
        let home = std::env::var("HOME").unwrap_or_default();
        Self {
            dir: orchestrator_dir(&env, &home),
        }
    }
    pub fn auth_path(&self) -> PathBuf {
        self.dir.join("auth.json")
    }
    pub fn machine_path(&self) -> PathBuf {
        self.dir.join("machine.json")
    }
    pub fn instances_path(&self) -> PathBuf {
        self.dir.join("instances.json")
    }
    pub fn socket_path(&self) -> PathBuf {
        self.dir.join("orchestrator.sock")
    }
}
