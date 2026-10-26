//! AWS credential and profile resolution for the Bedrock runtime client.
//!
//! Ports the observable subset of the AWS SDK Node default chain used by the
//! TypeScript module: environment credentials, shared config/credentials INI
//! profiles (static keys and SSO sessions backed by the SSO token cache), and
//! bearer-token authentication. Role assumption, `credential_process`, web
//! identity, ECS, and IMDS providers are explicit gaps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::api::bedrock_sigv4::AwsCredentials;
use crate::request::{BoxFuture, CallbackError, Fetch, HttpRequest};
use crate::types::ProviderEnv;
use crate::utils::provider_env::get_provider_env_value;

/// Resolve environment static credentials (`AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`).
pub fn env_credentials(env: Option<&ProviderEnv>) -> Option<AwsCredentials> {
    let access_key_id = get_provider_env_value("AWS_ACCESS_KEY_ID", env)?;
    let secret_access_key = get_provider_env_value("AWS_SECRET_ACCESS_KEY", env)?;
    Some(AwsCredentials {
        access_key_id,
        secret_access_key,
        session_token: get_provider_env_value("AWS_SESSION_TOKEN", env),
    })
}

/// `getProfileName`: explicit profile, then `AWS_PROFILE`, then `default`.
pub fn profile_name(explicit: Option<&JsString>, env: Option<&ProviderEnv>) -> JsString {
    explicit
        .cloned()
        .or_else(|| get_provider_env_value("AWS_PROFILE", env))
        .unwrap_or_else(|| "default".into())
}

/// Resolve credentials for a profile from shared files and the SSO cache.
pub fn profile_credentials(
    profile: &JsString,
    env: Option<&ProviderEnv>,
    fetch: std::sync::Arc<dyn Fetch>,
) -> BoxFuture<'static, Result<Option<AwsCredentials>, CallbackError>> {
    let profile = profile.clone();
    let env = env.cloned();
    Box::pin(async move {
        let files = SharedFiles::load(env.as_ref());
        let Some(section) = files.profile(&profile) else {
            return Ok(None);
        };
        if let Some(credentials) = static_credentials(&section) {
            return Ok(Some(credentials));
        }
        let Some(settings) = sso_settings(&files, &section) else {
            return Ok(None);
        };
        resolve_sso_credentials(&settings, fetch).await.map(Some)
    })
}

/// Region from the shared config file for `profile`.
pub fn profile_region(profile: &JsString, env: Option<&ProviderEnv>) -> Option<String> {
    let files = SharedFiles::load(env);
    files
        .profile(profile)
        .and_then(|section| section.get("region").cloned())
        .filter(|region| !region.is_empty())
}

fn static_credentials(section: &BTreeMap<String, String>) -> Option<AwsCredentials> {
    let access_key_id = section.get("aws_access_key_id").filter(|value| !value.is_empty())?;
    let secret_access_key = section.get("aws_secret_access_key").filter(|value| !value.is_empty())?;
    Some(AwsCredentials {
        access_key_id: access_key_id.into(),
        secret_access_key: secret_access_key.into(),
        session_token: section
            .get("aws_session_token")
            .filter(|value| !value.is_empty())
            .map(JsString::from),
    })
}

#[derive(Debug, Clone)]
struct SsoSettings {
    region: String,
    account_id: String,
    role_name: String,
    cache_key: String,
}

fn sso_settings(files: &SharedFiles, section: &BTreeMap<String, String>) -> Option<SsoSettings> {
    let (region, cache_key) = match section.get("sso_session") {
        Some(session_name) => {
            let session = files.sso_session(session_name)?;
            (session.get("sso_region")?.clone(), session_name.clone())
        }
        None => (
            section.get("sso_region")?.clone(),
            section.get("sso_start_url")?.clone(),
        ),
    };
    Some(SsoSettings {
        region,
        account_id: section.get("sso_account_id")?.clone(),
        role_name: section.get("sso_role_name")?.clone(),
        cache_key,
    })
}

async fn resolve_sso_credentials(
    settings: &SsoSettings,
    fetch: std::sync::Arc<dyn Fetch>,
) -> Result<AwsCredentials, CallbackError> {
    let token = read_sso_token(&settings.cache_key)?;
    let access_token = token
        .get("accessToken")
        .and_then(Value::as_str)
        .ok_or_else(|| CallbackError::new("The SSO session token is invalid"))?
        .to_owned();
    let expires_at = token
        .get("expiresAt")
        .and_then(Value::as_str)
        .ok_or_else(|| CallbackError::new("The SSO session token is invalid"))?;
    let expires_at_ms = pi_js_compat::date::date_parse(expires_at);
    if expires_at_ms.is_nan() {
        return Err(CallbackError::new("The SSO session token is invalid"));
    }
    if expires_at_ms <= crate::request::now_millis() as f64 {
        return Err(CallbackError::new(
            "The SSO session associated with this profile has expired. To refresh this SSO session run aws sso login with the corresponding profile.",
        ));
    }

    let url = format!(
        "https://portal.sso.{}/federation/credentials?role_name={}&account_id={}",
        settings.region,
        percent_encode(&settings.role_name),
        percent_encode(&settings.account_id)
    );
    let request = HttpRequest {
        method: "GET".to_owned(),
        url,
        headers: vec![("x-amz-sso_bearer_token".to_owned(), access_token)],
        body: None,
    };
    let response = fetch.fetch(request, None).await?;
    let status = response.status;
    let body = read_body(response.body).await?;
    if !(200..300).contains(&status) {
        return Err(CallbackError::new(format!(
            "SSO GetRoleCredentials failed with status {status}"
        )));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|error| CallbackError::new(format!("SSO response was not JSON: {error}")))?;
    let credentials = &value["roleCredentials"];
    let access_key_id = credentials["accessKeyId"]
        .as_str()
        .ok_or_else(|| CallbackError::new("SSO returns an invalid temporary credential."))?;
    let secret_access_key = credentials["secretAccessKey"]
        .as_str()
        .ok_or_else(|| CallbackError::new("SSO returns an invalid temporary credential."))?;
    let session_token = credentials["sessionToken"]
        .as_str()
        .ok_or_else(|| CallbackError::new("SSO returns an invalid temporary credential."))?;
    Ok(AwsCredentials {
        access_key_id: access_key_id.into(),
        secret_access_key: secret_access_key.into(),
        session_token: Some(session_token.into()),
    })
}

async fn read_body(mut body: Box<dyn crate::request::HttpBody>) -> Result<Vec<u8>, CallbackError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

fn read_sso_token(cache_key: &str) -> Result<Value, CallbackError> {
    let path = sso_token_path(cache_key)?;
    // readFile(ssoTokenFilepath, "utf8") in @smithy/core/config.
    let bytes = std::fs::read(&path).map_err(|_| {
        CallbackError::new(
            "The SSO session associated with this profile is invalid. To refresh this SSO session run aws sso login with the corresponding profile.",
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    pi_js_compat::js_json::parse(text.as_ref())
        .map_err(|error| CallbackError::new(format!("The SSO session token is invalid: {error}")))
}

fn sso_token_path(cache_key: &str) -> Result<PathBuf, CallbackError> {
    let home = pi_js_compat::process_env::get("HOME")
        .ok_or_else(|| CallbackError::new("HOME is not set; cannot read the SSO token cache"))?;
    let digest = Sha1::digest(cache_key.as_bytes());
    let mut name = String::with_capacity(40);
    for byte in digest {
        name.push(HEX[(byte >> 4) as usize] as char);
        name.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(Path::new(&home)
        .join(".aws")
        .join("sso")
        .join("cache")
        .join(format!("{name}.json")))
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

#[derive(Debug, Default)]
struct SharedFiles {
    profiles: BTreeMap<String, BTreeMap<String, String>>,
    sso_sessions: BTreeMap<String, BTreeMap<String, String>>,
}

impl SharedFiles {
    fn load(env: Option<&ProviderEnv>) -> Self {
        let home = pi_js_compat::process_env::get("HOME").unwrap_or_default();
        // Node encodes path strings as UTF-8 at the fs boundary.
        let config_path = get_provider_env_value("AWS_CONFIG_FILE", env)
            .map(|path| PathBuf::from(encode_node_utf8(&path)))
            .unwrap_or_else(|| Path::new(&home).join(".aws").join("config"));
        let credentials_path = get_provider_env_value("AWS_SHARED_CREDENTIALS_FILE", env)
            .map(|path| PathBuf::from(encode_node_utf8(&path)))
            .unwrap_or_else(|| Path::new(&home).join(".aws").join("credentials"));

        let mut files = Self::default();
        files.merge_config(&config_path);
        files.merge_credentials(&credentials_path);
        files
    }

    /// Section names decode from UTF-8 files and never hold unpaired
    /// surrogates, so a name with one matches no profile.
    fn profile(&self, name: &JsString) -> Option<BTreeMap<String, String>> {
        self.profiles.get(&name.to_utf8().ok()?).cloned()
    }

    fn sso_session(&self, name: &str) -> Option<BTreeMap<String, String>> {
        self.sso_sessions.get(name).cloned()
    }

    fn merge_config(&mut self, path: &Path) {
        let Some(sections) = parse_ini(path) else {
            return;
        };
        for (section, values) in sections {
            if let Some(name) = section.strip_prefix("profile ") {
                self.profiles.entry(name.to_owned()).or_default().extend(values);
            } else if let Some(name) = section.strip_prefix("sso-session ") {
                self.sso_sessions.entry(name.to_owned()).or_default().extend(values);
            } else if section == "default" {
                self.profiles.entry(section).or_default().extend(values);
            }
        }
    }

    fn merge_credentials(&mut self, path: &Path) {
        let Some(sections) = parse_ini(path) else {
            return;
        };
        for (section, values) in sections {
            self.profiles.entry(section).or_default().extend(values);
        }
    }
}

fn parse_ini(path: &Path) -> Option<Vec<(String, BTreeMap<String, String>)>> {
    // readFile(path, "utf8") in @smithy/core shared-ini-file-loader.
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut sections: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;
    for line in text.lines() {
        let trimmed = strip_comment(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(section) = current.take()
                && let Some(last) = sections.last_mut()
            {
                last.1.extend(section);
            }
            let name = trimmed[1..trimmed.len() - 1].trim().to_owned();
            sections.push((name, BTreeMap::new()));
            current = Some(BTreeMap::new());
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if let Some(section) = current.as_mut() {
            section.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    if let Some(section) = current
        && let Some(last) = sections.last_mut()
    {
        last.1.extend(section);
    }
    Some(sections)
}

fn strip_comment(line: &str) -> &str {
    let mut result = line;
    for (index, character) in line.char_indices() {
        if character == '#' || character == ';' {
            let previous = line[..index].chars().next_back();
            if previous.is_none_or(char::is_whitespace) {
                result = &line[..index];
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod utf8_tests {
    use super::*;

    #[test]
    fn shared_ini_replaces_invalid_utf8() {
        let path = std::env::temp_dir().join(format!("utf8-ini-{}", std::process::id()));
        std::fs::write(
            &path,
            b"[default]\nregion=us-east-1\nnote=before\xe2\x82after\xff\n[profile retained]\nregion=eu-west-1\n",
        )
        .unwrap();
        let sections = parse_ini(&path);
        std::fs::remove_file(&path).unwrap();
        let sections = sections.unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].1["region"], "us-east-1");
        assert_eq!(sections[0].1["note"], "before\u{fffd}after\u{fffd}");
        assert_eq!(sections[1].1["region"], "eu-west-1");
    }

    #[test]
    fn sso_token_replaces_invalid_utf8() {
        const CHILD: &str = "PI_UTF8_SSO_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let path = sso_token_path("utf8-sweep").unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                b"{\"accessToken\":\"kept\",\"note\":\"before\xe2\x82after\xff\"}",
            )
            .unwrap();
            let token = read_sso_token("utf8-sweep").unwrap();
            assert_eq!(token["accessToken"], "kept");
            assert_eq!(token["note"], "before\u{fffd}after\u{fffd}");
            return;
        }
        let home = std::env::temp_dir().join(format!("utf8-sso-{}", std::process::id()));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "api::bedrock_credentials::utf8_tests::sso_token_replaces_invalid_utf8",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", &home)
            .status()
            .unwrap();
        std::fs::remove_dir_all(&home).unwrap();
        assert!(status.success());
    }
}
