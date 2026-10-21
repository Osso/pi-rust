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
    join_path(&pi_dir, "orchestrator")
}

fn join_path(base: &std::path::Path, name: &str) -> PathBuf {
    let combined = format!("{}/{name}", base.display());
    let absolute = combined.starts_with('/');
    let mut segments = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|s| *s != "..") => {
                segments.pop();
            }
            ".." if absolute => {}
            segment => segments.push(segment),
        }
    }
    let relative = segments.join("/");
    PathBuf::from(if absolute { format!("/{relative}") } else { relative })
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
        join_path(&self.dir, "auth.json")
    }
    pub fn machine_path(&self) -> PathBuf {
        join_path(&self.dir, "machine.json")
    }
    pub fn instances_path(&self) -> PathBuf {
        join_path(&self.dir, "instances.json")
    }
    pub fn socket_path(&self) -> PathBuf {
        join_path(&self.dir, "orchestrator.sock")
    }
}
