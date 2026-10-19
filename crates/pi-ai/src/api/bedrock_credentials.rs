//! AWS credential and profile resolution for the Bedrock runtime client.
//!
//! Ports the observable subset of the AWS SDK Node default chain used by the
//! TypeScript module: environment credentials, shared config/credentials INI
//! profiles (static keys and SSO sessions backed by the SSO token cache), and
//! bearer-token authentication. Role assumption, `credential_process`, web
//! identity, ECS, and IMDS providers are explicit gaps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
pub fn profile_name(explicit: Option<&str>, env: Option<&ProviderEnv>) -> String {
    explicit
        .map(str::to_owned)
        .or_else(|| get_provider_env_value("AWS_PROFILE", env))
        .unwrap_or_else(|| "default".to_owned())
}

/// Resolve credentials for a profile from shared files and the SSO cache.
pub fn profile_credentials(
    profile: &str,
    env: Option<&ProviderEnv>,
    fetch: std::sync::Arc<dyn Fetch>,
) -> BoxFuture<'static, Result<Option<AwsCredentials>, CallbackError>> {
    let profile = profile.to_owned();
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
pub fn profile_region(profile: &str, env: Option<&ProviderEnv>) -> Option<String> {
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
        access_key_id: access_key_id.clone(),
        secret_access_key: secret_access_key.clone(),
        session_token: section
            .get("aws_session_token")
            .filter(|value| !value.is_empty())
            .cloned(),
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
        access_key_id: access_key_id.to_owned(),
        secret_access_key: secret_access_key.to_owned(),
        session_token: Some(session_token.to_owned()),
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
    let text = std::fs::read_to_string(&path).map_err(|_| {
        CallbackError::new(
            "The SSO session associated with this profile is invalid. To refresh this SSO session run aws sso login with the corresponding profile.",
        )
    })?;
    pi_js_compat::js_json::parse(&text)
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
        let config_path = get_provider_env_value("AWS_CONFIG_FILE", env)
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&home).join(".aws").join("config"));
        let credentials_path = get_provider_env_value("AWS_SHARED_CREDENTIALS_FILE", env)
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&home).join(".aws").join("credentials"));

        let mut files = Self::default();
        files.merge_config(&config_path);
        files.merge_credentials(&credentials_path);
        files
    }

    fn profile(&self, name: &str) -> Option<BTreeMap<String, String>> {
        self.profiles.get(name).cloned()
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
    let text = std::fs::read_to_string(path).ok()?;
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
