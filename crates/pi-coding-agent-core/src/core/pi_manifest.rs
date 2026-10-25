//! `package.json` `pi` manifest reader (`core/pi-manifest.ts`).

use pi_js_compat::js_json::{JsObject, JsString, encode_node_utf8, parse_lossless};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub themes: Option<Vec<JsString>>,
}

/// Read the `pi` field; the source explicitly returns null for read/parse errors.
pub fn read_pi_manifest(package_json_path: &JsString) -> Option<PiManifest> {
    let bytes = match std::fs::read(encode_node_utf8(package_json_path)) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    // Node's readFileSync(path, "utf-8") replaces invalid UTF-8 bytes at this boundary.
    let package = match parse_lossless(String::from_utf8_lossy(&bytes).as_ref()) {
        Ok(package) => package,
        Err(_) => return None,
    };
    let pi = package.as_object()?.get("pi")?.as_object()?;
    Some(PiManifest {
        extensions: string_array(pi, "extensions"),
        skills: string_array(pi, "skills"),
        prompts: string_array(pi, "prompts"),
        themes: string_array(pi, "themes"),
    })
}

fn string_array(pi: &JsObject, field: &str) -> Option<Vec<JsString>> {
    pi.get(field)?
        .as_array()?
        .iter()
        .map(|entry| entry.as_string().cloned())
        .collect()
}
