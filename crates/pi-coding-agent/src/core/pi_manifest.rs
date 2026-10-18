//! `package.json` `pi` manifest reader (`core/pi-manifest.ts`).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub themes: Option<Vec<String>>,
}

/// Read the `pi` field of a `package.json`; `None` when unreadable, invalid, or absent.
pub fn read_pi_manifest(package_json_path: &Path) -> Option<PiManifest> {
    let bytes = std::fs::read(package_json_path).ok()?;
    // `readFileSync(path, "utf-8")` decodes lossily.
    let package: Value = serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok()?;
    let pi = package.as_object()?.get("pi")?.as_object()?;
    Some(PiManifest {
        extensions: string_array(pi, "extensions"),
        skills: string_array(pi, "skills"),
        prompts: string_array(pi, "prompts"),
        themes: string_array(pi, "themes"),
    })
}

fn string_array(pi: &Map<String, Value>, field: &str) -> Option<Vec<String>> {
    pi.get(field)?
        .as_array()?
        .iter()
        .map(|entry| entry.as_str().map(str::to_owned))
        .collect()
}
