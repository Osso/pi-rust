//! Port of `packages/coding-agent/src/utils/version-check.ts`.
//!
//! Version comparison follows `node-semver`'s strict parser and precedence rules; the release check
//! goes through the injected `Fetch` contract.

use pi_ai::request::{CallbackError, Fetch, HttpRequest, header_value};
use pi_js_compat::{
    abort::AbortSignal,
    js_json::{self, JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};

use crate::config::package_config;
use crate::utils::pi_user_agent::get_pi_user_agent;

const LATEST_VERSION_URL: &str = "https://pi.dev/api/latest-version";
const DEFAULT_VERSION_CHECK_TIMEOUT_MS: f64 = 10_000.0;
const MAX_SEMVER_LENGTH: usize = 256;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// A parsed strict semver version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
    pub build: Vec<String>,
}

impl Semver {
    pub fn version(&self) -> String {
        let mut version = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.prerelease.is_empty() {
            version.push('-');
            version.push_str(&self.prerelease.join("."));
        }
        version
    }
}

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_numeric_identifier(identifier: &str) -> bool {
    if !is_numeric_identifier(identifier) {
        return false;
    }
    identifier == "0" || !identifier.starts_with('0')
}

fn valid_prerelease_identifier(identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    if is_numeric_identifier(identifier) {
        return valid_numeric_identifier(identifier);
    }
    identifier
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_build_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// `semver.valid`: strict parse, `v` prefix allowed, leading zeros rejected.
pub fn parse_semver(version: &JsString) -> Option<Semver> {
    let trimmed = version.trim();
    // Valid semver is ASCII; unmatched units cannot match the grammar.
    let scalar = match trimmed.to_utf8() {
        Ok(scalar) => scalar,
        Err(_) => return None,
    };
    let trimmed = scalar.as_str();
    if trimmed.len() > MAX_SEMVER_LENGTH {
        return None;
    }
    let body = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let (without_build, build) = split_once_optional(body, '+');
    let (core, prerelease) = split_once_optional(without_build, '-');
    let (major, minor, patch) = parse_core_version(core)?;
    Some(Semver {
        major,
        minor,
        patch,
        prerelease: parse_prerelease(prerelease)?,
        build: parse_build_metadata(build)?,
    })
}

fn split_once_optional(value: &str, separator: char) -> (&str, Option<&str>) {
    match value.split_once(separator) {
        Some((head, tail)) => (head, Some(tail)),
        None => (value, None),
    }
}

fn parse_core_version(core: &str) -> Option<(u64, u64, u64)> {
    let mut parts = core.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if !valid_numeric_identifier(major) || !valid_numeric_identifier(minor) || !valid_numeric_identifier(patch) {
        return None;
    }
    Some((
        parse_safe_number(major)?,
        parse_safe_number(minor)?,
        parse_safe_number(patch)?,
    ))
}

fn parse_safe_number(value: &str) -> Option<u64> {
    value.parse().ok().filter(|value| *value <= MAX_SAFE_INTEGER)
}

fn parse_prerelease(prerelease: Option<&str>) -> Option<Vec<String>> {
    let Some(prerelease) = prerelease else {
        return Some(Vec::new());
    };
    let identifiers: Vec<String> = prerelease.split('.').map(str::to_owned).collect();
    identifiers
        .iter()
        .all(|identifier| valid_prerelease_identifier(identifier))
        .then_some(identifiers)
}

fn parse_build_metadata(build: Option<&str>) -> Option<Vec<String>> {
    let Some(build) = build else {
        return Some(Vec::new());
    };
    let identifiers: Vec<String> = build.split('.').map(str::to_owned).collect();
    identifiers
        .iter()
        .all(|identifier| valid_build_identifier(identifier))
        .then_some(identifiers)
}

fn compare_identifiers(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let left_numeric = is_numeric_identifier(left);
    let right_numeric = is_numeric_identifier(right);
    match (left_numeric, right_numeric) {
        (true, true) => {
            let left: u64 = left.parse().unwrap_or(0);
            let right: u64 = right.parse().unwrap_or(0);
            left.cmp(&right)
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(right),
    }
}

/// `semver.compare`: version precedence.
pub fn compare_semver(left: &Semver, right: &Semver) -> i32 {
    use std::cmp::Ordering;
    let main = (left.major, left.minor, left.patch).cmp(&(right.major, right.minor, right.patch));
    if main != Ordering::Equal {
        return if main == Ordering::Less { -1 } else { 1 };
    }
    match (left.prerelease.is_empty(), right.prerelease.is_empty()) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => -1,
        (false, false) => {
            for (left_identifier, right_identifier) in left.prerelease.iter().zip(right.prerelease.iter()) {
                match compare_identifiers(left_identifier, right_identifier) {
                    Ordering::Equal => {}
                    Ordering::Less => return -1,
                    Ordering::Greater => return 1,
                }
            }
            match left.prerelease.len().cmp(&right.prerelease.len()) {
                Ordering::Equal => 0,
                Ordering::Less => -1,
                Ordering::Greater => 1,
            }
        }
    }
}

/// `comparePackageVersions`: `None` when either side is not a valid version.
pub fn compare_package_versions(left_version: &JsString, right_version: &JsString) -> Option<i32> {
    let left = parse_semver(left_version)?;
    let right = parse_semver(right_version)?;
    Some(compare_semver(&left, &right))
}

/// `isNewerPackageVersion`: semver comparison, falling back to trimmed string inequality.
pub fn is_newer_package_version(candidate_version: &JsString, current_version: &JsString) -> bool {
    match compare_package_versions(candidate_version, current_version) {
        Some(comparison) => comparison > 0,
        None => candidate_version.trim() != current_version.trim(),
    }
}

/// `LatestPiRelease`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestPiRelease {
    pub version: JsString,
    pub package_name: Option<JsString>,
    pub note: Option<JsString>,
}

/// Options for the release check.
#[derive(Debug, Clone, Default)]
pub struct LatestPiReleaseOptions {
    pub timeout_ms: Option<f64>,
}

fn read_version_check_disabled() -> bool {
    ["PI_SKIP_VERSION_CHECK", "PI_OFFLINE"]
        .iter()
        .any(|name| pi_js_compat::process_env::get(name).is_some_and(|value| !value.is_empty()))
}

/// `getLatestPiRelease`: one request; failures propagate to the caller.
pub async fn get_latest_pi_release(
    fetch: &dyn Fetch,
    current_version: &JsString,
    options: &LatestPiReleaseOptions,
) -> Result<Option<LatestPiRelease>, CallbackError> {
    if read_version_check_disabled() {
        return Ok(None);
    }
    let signal = AbortSignal::timeout(options.timeout_ms.unwrap_or(DEFAULT_VERSION_CHECK_TIMEOUT_MS))?;
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".to_owned(),
                url: LATEST_VERSION_URL.to_owned(),
                headers: vec![
                    (
                        "user-agent".to_owned(),
                        header_value(&get_pi_user_agent(current_version))?,
                    ),
                    ("accept".to_owned(), "application/json".to_owned()),
                ],
                body: None,
            },
            Some(signal),
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Ok(None);
    }
    let body = read_body(response.body).await?;
    parse_release_payload(&body)
}

fn parse_release_payload(body: &JsString) -> Result<Option<LatestPiRelease>, CallbackError> {
    let data = js_json::parse_lossless(body)
        .map_err(|error| NativeNodeError::from_constructor(NativeErrorClass::SyntaxError, error.into_message()))?;
    let Some(version) = data.get("version").and_then(JsValue::as_string) else {
        return Ok(None);
    };
    let version = version.trim().to_owned();
    if version.is_empty() {
        return Ok(None);
    }
    let package_name = trimmed_string_field(&data, "packageName");
    let note = trimmed_string_field(&data, "note");
    Ok(Some(LatestPiRelease {
        version,
        package_name,
        note,
    }))
}

fn trimmed_string_field(data: &JsValue, field: &str) -> Option<JsString> {
    data.get(field)
        .and_then(JsValue::as_string)
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
}

/// `getLatestPiVersion`.
pub async fn get_latest_pi_version(
    fetch: &dyn Fetch,
    current_version: &JsString,
    options: &LatestPiReleaseOptions,
) -> Result<Option<JsString>, CallbackError> {
    Ok(get_latest_pi_release(fetch, current_version, options)
        .await?
        .map(|release| release.version))
}

/// `checkForNewPiVersion`: automatic newer-only check; errors are swallowed here alone.
pub async fn check_for_new_pi_version(fetch: &dyn Fetch, current_version: &JsString) -> Option<LatestPiRelease> {
    let release = get_latest_pi_release(fetch, current_version, &LatestPiReleaseOptions::default())
        .await
        .ok()??;
    is_newer_package_version(&release.version, current_version).then_some(release)
}

async fn read_body(mut body: Box<dyn pi_ai::request::HttpBody>) -> Result<JsString, CallbackError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    let decoded = String::from_utf8_lossy(&bytes);
    // Response.json() uses UTF-8 decoding with one leading BOM removed.
    Ok(JsString::from(decoded.strip_prefix('\u{feff}').unwrap_or(&decoded)))
}

/// User agent used for version checks.
pub fn version_check_user_agent(current_version: &JsString) -> JsString {
    get_pi_user_agent(current_version)
}

/// Package version used by default callers.
pub fn current_package_version() -> JsString {
    package_config().version.clone()
}

#[cfg(test)]
mod native_release_tests {
    use super::*;
    #[test]
    fn release_json_preserves_unpaired_version_package_and_note() {
        let release = parse_release_payload(&JsString::from(
            r#"{"version":"\ud800","packageName":"pkg\udc00","note":"note\ud800"}"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(release.version.units(), &[0xd800]);
        assert_eq!(release.package_name.unwrap().units(), &[112, 107, 103, 0xdc00]);
        assert_eq!(release.note.unwrap().units(), &[110, 111, 116, 101, 0xd800]);
    }
}
