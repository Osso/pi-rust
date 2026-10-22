//! Frozen orchestrator path and package metadata configuration.
use pi_coding_agent::utils::paths::node_join;
use pi_js_compat::js_json::JsString;
use std::collections::BTreeMap;

pub const VERSION: &str = "0.80.3";
pub const IS_BUN_BINARY: bool = false;

pub fn orchestrator_dir(env: &BTreeMap<String, JsString>, home: &JsString) -> JsString {
    if let Some(dir) = env.get("PI_ORCHESTRATOR_DIR").filter(|v| !v.is_empty()) {
        return dir.clone();
    }
    let pi_dir = env
        .get("PI_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| node_join(home, ".pi"));
    node_join(pi_dir, "orchestrator")
}

#[derive(Debug, Clone)]
pub struct Config {
    pub dir: JsString,
}
impl Config {
    pub fn from_environment() -> Self {
        let env = std::env::vars()
            .map(|(key, value)| (key, JsString::from(value)))
            .collect();
        let home = std::env::home_dir().expect("OS home directory unavailable");
        let home = JsString::from(home.to_str().expect("OS home directory must be UTF-8"));
        Self {
            dir: orchestrator_dir(&env, &home),
        }
    }
    pub fn auth_path(&self) -> JsString {
        node_join(&self.dir, "auth.json")
    }
    pub fn machine_path(&self) -> JsString {
        node_join(&self.dir, "machine.json")
    }
    pub fn instances_path(&self) -> JsString {
        node_join(&self.dir, "instances.json")
    }
    pub fn socket_path(&self) -> JsString {
        node_join(&self.dir, "orchestrator.sock")
    }
}
