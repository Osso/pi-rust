//! Source auth-trace.ts: private metadata-only credential observations.
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
};
use std::path::PathBuf;
#[derive(Clone)]
pub(super) struct AuthProfile {
    pub home: PathBuf,
    pub name: String,
}
pub(super) fn fingerprint(token: &JsString) -> String {
    let text = pi_js_compat::js_json::encode_node_utf8(token);
    crate::core::detached_job_runner::sha256_hex(text.as_bytes())[..16].into()
}
pub(super) fn record(profile: &AuthProfile, event: &str, source: Option<&str>) -> Result<(), CallbackError> {
    append_trace(profile, event, source, None)
}
pub(super) fn record_failure(profile: &AuthProfile, text: &JsString) -> Result<(), CallbackError> {
    if let Some(failure) = classify_failure(text) {
        append_trace(profile, "oauth_failure", None, Some(failure))?;
    }
    Ok(())
}
fn classify_failure(text: &JsString) -> Option<JsValue> {
    use std::sync::LazyLock;
    static PATTERNS: LazyLock<Vec<(regex::Regex, &str)>> = LazyLock::new(|| {
        [
            (r"(?i)invalid_grant", "invalid_grant"),
            (r"(?i)lock compromised", "lock_compromised"),
            (
                r"(?i)another.*process.*refresh|refresh.*lock.*(?:timeout|contended)",
                "lock_contended",
            ),
            (
                r"(?i)failed to save oauth|save.*oauth.*(?:failed|exception)|oauth.*save.*failed",
                "save_failed",
            ),
            (
                r"(?i)session expired.*could not be refreshed|no.refresh.token",
                "refresh_unavailable",
            ),
            (r"(?i)refresh.*failed|failed.*refresh", "refresh_failed"),
            (
                r"(?i)failed to authenticate|authentication.failed",
                "authentication_failed",
            ),
        ]
        .into_iter()
        .map(|(pattern, kind)| (regex::Regex::new(pattern).expect("static auth pattern"), kind))
        .collect()
    });
    static RELEVANT: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)oauth|authenticat|refresh token").expect("static relevant pattern"));
    static STATUS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b(?:HTTP|status(?: code)?)\s*[:=]?\s*([45]\d{2})\b").expect("static status pattern")
    });
    let text = pi_js_compat::js_json::encode_node_utf8(text);
    if !RELEVANT.is_match(&text) {
        return None;
    }
    let (_, kind) = PATTERNS.iter().find(|(pattern, _)| pattern.is_match(&text))?;
    let mut fields = pi_js_compat::js_json::JsObject::new();
    fields.insert("kind".into(), JsValue::String((*kind).into()));
    if let Some(status) = STATUS
        .captures(&text)
        .and_then(|m| m.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
    {
        fields.insert("httpStatus".into(), JsValue::Number(status));
    }
    Some(JsValue::Object(fields))
}
fn credential_metadata(content: &str) -> JsValue {
    use pi_js_compat::js_json::{self, JsObject};
    let Ok(value) = js_json::parse_lossless(content) else {
        return JsValue::object([("state", JsValue::String("invalid_json".into()))]);
    };
    let oauth = value.get("claudeAiOauth");
    let token = |key| {
        oauth
            .and_then(|o| o.get(key))
            .and_then(JsValue::as_string)
            .filter(|s| !s.is_empty())
    };
    let access = token("accessToken");
    let refresh = token("refreshToken");
    let mut fields = JsObject::new();
    fields.insert("state".into(), JsValue::String("present".into()));
    fields.insert("accessTokenPresent".into(), JsValue::Bool(access.is_some()));
    fields.insert("refreshTokenPresent".into(), JsValue::Bool(refresh.is_some()));
    if let Some(token) = access {
        fields.insert("accessFingerprint".into(), JsValue::String(fingerprint(token).into()));
    }
    if let Some(token) = refresh {
        fields.insert("refreshFingerprint".into(), JsValue::String(fingerprint(token).into()));
    }
    for key in ["expiresAt", "refreshTokenExpiresAt"] {
        if let Some(number) = oauth
            .and_then(|o| o.get(key))
            .and_then(JsValue::as_f64)
            .filter(|n| n.is_finite())
        {
            fields.insert(key.into(), JsValue::Number(number));
        }
    }
    JsValue::Object(fields)
}
fn read_credential_file(path: &std::path::Path) -> JsValue {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let read = || -> std::io::Result<JsValue> {
        let mut file = std::fs::File::open(path)?;
        let stat = file.metadata()?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let mut fields = credential_metadata(&content).as_object().cloned().unwrap_or_default();
        fields.insert(
            "modifiedAt".into(),
            JsValue::Number(stat.mtime() as f64 * 1000.0 + stat.mtime_nsec() as f64 / 1_000_000.0),
        );
        fields.insert("inode".into(), JsValue::Number(stat.ino() as f64));
        Ok(JsValue::Object(fields))
    };
    read().unwrap_or_else(|error| read_error(&error))
}
fn read_error(error: &std::io::Error) -> JsValue {
    if error.kind() == std::io::ErrorKind::NotFound {
        return JsValue::object([("state", JsValue::String("missing".into()))]);
    }
    JsValue::object([
        ("state", JsValue::String("unreadable".into())),
        (
            "errorCode",
            JsValue::String(pi_js_compat::node_fs::node_error_code(error).into()),
        ),
    ])
}
fn profile_files(profile: &AuthProfile) -> JsValue {
    let original = profile.home.join(".claude");
    let active = match std::fs::read_to_string(original.join(".active-profile")) {
        Ok(name) if name.trim() == profile.name => read_credential_file(&original.join(".credentials.json")),
        Ok(_) => JsValue::object([("state", JsValue::String("unassociated".into()))]),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            JsValue::object([("state", JsValue::String("unassociated".into()))])
        }
        Err(error) => read_error(&error),
    };
    JsValue::object([
        (
            "named",
            read_credential_file(&original.join(format!("{}.credentials.json", profile.name))),
        ),
        (
            "isolated",
            read_credential_file(
                &profile
                    .home
                    .join(".config/pi-claude-bridge/profiles")
                    .join(&profile.name)
                    .join(".credentials.json"),
            ),
        ),
        ("active", active),
    ])
}
fn append_trace(
    profile: &AuthProfile,
    event: &str,
    source: Option<&str>,
    failure: Option<JsValue>,
) -> Result<(), CallbackError> {
    use pi_js_compat::js_json::{self, JsObject};
    use std::sync::{LazyLock, Mutex};
    static OBSERVATIONS: LazyLock<Mutex<std::collections::HashMap<String, String>>> =
        LazyLock::new(|| Mutex::new(Default::default()));
    let write = || -> Result<(), CallbackError> {
        let mut observation = JsObject::new();
        observation.insert("event".into(), JsValue::String(event.into()));
        if let Some(source) = source {
            observation.insert("source".into(), JsValue::String(source.into()));
        }
        if let Some(failure) = &failure {
            observation.insert("failure".into(), failure.clone());
        }
        observation.insert("files".into(), profile_files(profile));
        let kind = failure
            .as_ref()
            .and_then(|f| f.get("kind"))
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_default();
        let key = format!(
            "{}/{}/{event}/{}",
            profile.home.display(),
            profile.name,
            js_json::encode_node_utf8(&kind)
        );
        let signature = js_json::stringify_lossless(&JsValue::Object(observation.clone()));
        let mut observations = OBSERVATIONS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if observations.get(&key) == Some(&signature) {
            return Ok(());
        }
        let mut entry = JsObject::new();
        entry.insert("timestamp".into(), JsValue::String(super::logging::timestamp()?.into()));
        entry.insert("pid".into(), JsValue::Number(std::process::id().into()));
        entry.insert("profile".into(), JsValue::String(profile.name.as_str().into()));
        entry.extend(observation);
        append_private(
            &super::logging::agent_dir()?.join("claude-bridge-auth.jsonl"),
            &JsValue::Object(entry),
        )
        .map_err(|e| CallbackError::new(pi_js_compat::node_fs::node_error_code(&e)))?;
        observations.insert(key, signature);
        Ok(())
    };
    if let Err(error) = write() {
        let code = error
            .message
            .to_utf8()
            .ok()
            .filter(|code| !code.is_empty() && code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
            .unwrap_or_else(|| "unknown".into());
        eprintln!("Cannot write Claude authentication trace ({code})");
    }
    Ok(())
}
fn append_private(path: &std::path::Path, value: &JsValue) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all((pi_js_compat::js_json::stringify_lossless(value) + "\n").as_bytes())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_bridge_auth_trace_matches_frozen_metadata_and_failure_records() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let credentials = home.join(".claude/trace-fixture.credentials.json");
        std::fs::write(
            &credentials,
            r#"{"claudeAiOauth":{"accessToken":"abc","refreshToken":"refresh-secret","expiresAt":1000}}"#,
        )
        .unwrap();
        let file = std::fs::File::options().write(true).open(&credentials).unwrap();
        file.set_times(
            std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000)),
        )
        .unwrap();
        let profile = AuthProfile {
            home: home.clone(),
            name: "trace-fixture".into(),
        };
        record(&profile, "prepare", Some("named")).unwrap();
        record(&profile, "prepare", Some("named")).unwrap();
        record_failure(&profile, &"OAuth invalid_grant HTTP 401".into()).unwrap();
        let path = super::super::logging::agent_dir()
            .unwrap()
            .join("claude-bridge-auth.jsonl");
        let native = std::fs::read_to_string(&path).unwrap();
        assert!(!native.contains("refresh-secret"));
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let native = native
            .lines()
            .map(|s| pi_js_compat::js_json::parse_lossless(s).unwrap())
            .filter(|v| {
                v.get("profile")
                    .and_then(JsValue::as_string)
                    .is_some_and(|s| s == "trace-fixture")
            })
            .collect::<Vec<_>>();
        assert_eq!(native.len(), 2);
        let module = dir.path().join("auth-trace.mjs");
        std::fs::write(&module, include_str!("fixtures/auth-trace.mjs")).unwrap();
        let script = r#"
const fs=require('fs'),vm=require('vm');
(async()=>{const m=new vm.SourceTextModule(fs.readFileSync(process.argv[1],'utf8'));await m.link(spec=>{
const values=spec==='@earendil-works/pi-coding-agent'?{getAgentDir:()=>process.argv[3]}:require(spec);
return new vm.SyntheticModule(Object.keys(values),function(){for(const [key,value] of Object.entries(values))this.setExport(key,value);});});await m.evaluate();
const profile={home:process.argv[2],name:'trace-fixture'};m.namespace.recordCredentialTrace(profile,'prepare','named');m.namespace.recordCredentialTrace(profile,'prepare','named');m.namespace.recordAuthFailure(profile,'OAuth invalid_grant HTTP 401');process.stdout.write(fs.readFileSync(process.argv[3]+'/claude-bridge-auth.jsonl','utf8'));
})().catch(e=>{console.error(e);process.exitCode=1;});"#;
        let output = std::process::Command::new("node")
            .args(["--experimental-vm-modules", "-e", script])
            .arg(module)
            .arg(home)
            .arg(dir.path().join("oracle"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
        let expected = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| pi_js_compat::js_json::parse_lossless(line).unwrap())
            .collect::<Vec<_>>();
        let normalize = |mut value: JsValue| {
            if let JsValue::Object(fields) = &mut value {
                fields.shift_remove("timestamp");
                fields.shift_remove("pid");
            }
            value
        };
        assert_eq!(
            native.into_iter().map(normalize).collect::<Vec<_>>(),
            expected.into_iter().map(normalize).collect::<Vec<_>>()
        );
    }

    #[test]
    fn claude_bridge_auth_fingerprints_match_node_sha256_without_tokens() {
        assert_eq!(fingerprint(&"abc".into()), "ba7816bf8f01cfea");
        // Node hashes its replacement UTF-8 encoding for an unpaired unit.
        let token = JsString::from_utf16(vec![0xd800]);
        let node=std::process::Command::new("node").args(["-e",r#"process.stdout.write(require('crypto').createHash('sha256').update('\ud800').digest('hex').slice(0,16))"#]).output().unwrap();
        assert!(node.status.success());
        assert_eq!(fingerprint(&token), String::from_utf8(node.stdout).unwrap());
    }
}
