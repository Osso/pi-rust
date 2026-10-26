//! The versioned cache is shared with the TypeScript extension.
use super::{data, parser};
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_fs::{decode_node_path, node_fs_error};
use pi_js_compat::number::{js_number_to_string, math_round};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

pub const VERSION: &str = "0.2.1";

#[derive(Clone, Debug, Default)]
pub struct LoadingProgress {
    pub total: usize,
    pub done: usize,
    pub pct: f64,
    pub remaining_time_ms: Option<f64>,
}

pub fn read_cache(path: &Path) -> Option<JsValue> {
    let raw = std::fs::read(path).ok()?;
    let payload = js_json::parse_lossless(String::from_utf8_lossy(&raw).as_ref()).ok()?;
    let signature = payload.get("signature")?;
    if !js_json::is_truthy(signature) || !payload["sessions"].is_array() {
        return None;
    }
    Some(payload)
}

pub fn write_cache(path: &Path, signature: &JsString, sessions: &[JsValue], now: f64) -> Result<(), JsString> {
    let timestamp = pi_js_compat::date::to_iso_string_value(now).map_err(|error| JsString::from(error.to_string()))?;
    let payload = JsValue::object([
        ("version", JsValue::String(VERSION.into())),
        ("signature", JsValue::String(signature.clone())),
        ("generatedAt", JsValue::String(timestamp.into())),
        ("sessions", JsValue::Array(sessions.to_vec())),
    ]);
    std::fs::write(path, js_json::stringify_lossless(&payload))
        .map_err(|error| node_fs_error(&error, "open", &decode_node_path(path.as_os_str())))
}

fn find_logs(dir: &Path, output: &mut Vec<PathBuf>) -> Result<(), JsString> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| node_fs_error(&error, "scandir", &decode_node_path(dir.as_os_str())))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| node_fs_error(&error, "stat", &decode_node_path(path.as_os_str())))?;
        if kind.is_dir() {
            find_logs(&path, output)?;
        } else if kind.is_file() && entry.file_name().as_encoded_bytes().ends_with(b".jsonl") {
            output.push(path);
        }
    }
    Ok(())
}

pub fn compute_signature(dir: &Path) -> Result<JsString, JsString> {
    let mut files = Vec::new();
    find_logs(dir, &mut files)?;
    if files.is_empty() {
        return Ok(JsString::default());
    }
    files.sort_by(|left, right| {
        pi_js_compat::collator::locale_compare_utf16(
            decode_node_path(left.as_os_str()).units(),
            decode_node_path(right.as_os_str()).units(),
        )
    });
    let mut signature_input = String::new();
    for path in files {
        let metadata = std::fs::metadata(&path)
            .map_err(|error| node_fs_error(&error, "stat", &decode_node_path(path.as_os_str())))?;
        let modified = metadata.modified().map_err(|error| JsString::from(error.to_string()))?;
        let millis = match modified.duration_since(UNIX_EPOCH) {
            Ok(time) => time.as_secs() as f64 * 1000.0 + f64::from(time.subsec_nanos()) / 1_000_000.0,
            Err(error) => {
                let time = error.duration();
                -(time.as_secs() as f64 * 1000.0 + f64::from(time.subsec_nanos()) / 1_000_000.0)
            }
        };
        signature_input.push_str(&js_json::encode_node_utf8(&decode_node_path(path.as_os_str())));
        signature_input.push('\n');
        signature_input.push_str(&js_number_to_string(metadata.len() as f64));
        signature_input.push('\n');
        signature_input.push_str(&js_number_to_string(millis));
        signature_input.push('\n');
    }
    Ok(crate::core::detached_job_runner::sha256_hex(signature_input.as_bytes()).into())
}

pub fn load_aggregate(cache: &Path, sessions: &Path, now: f64) -> Result<Vec<JsValue>, JsString> {
    load_aggregate_with_progress(cache, sessions, now, false, &mut |_| {}, &mut |_| {})
}

pub fn load_aggregate_with_progress(
    cache: &Path,
    sessions: &Path,
    now: f64,
    force: bool,
    on_progress: &mut dyn FnMut(LoadingProgress),
    on_log: &mut dyn FnMut(JsString),
) -> Result<Vec<JsValue>, JsString> {
    let force_cache = pi_js_compat::process_env::get("PI_ATLAS_FORCE_CACHE")
        .and_then(|value| value.parse::<f64>().ok())
        .is_some_and(|value| value != 0.0 && !value.is_nan());
    if !force
        && !force_cache
        && let Some(cached) = read_cache(cache)
        && cached["version"].as_string().is_some_and(|version| version == VERSION)
        && let Some(second_read) = read_cache(cache)
        && second_read["signature"].as_string() == Some(&compute_signature(sessions)?)
    {
        return Ok(cached["sessions"].as_array().expect("validated cache sessions").clone());
    }
    let mut files = Vec::new();
    find_logs(sessions, &mut files)?;
    let started = Instant::now();
    on_progress(progress(files.len(), 0, 0.0));
    let mut result = Vec::new();
    let mut corrupt = 0;
    let delay = pi_js_compat::process_env::get("PI_ATLAS_SLOW_DELAY_MS")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    for (index, file) in files.iter().enumerate() {
        if delay > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(delay / 1000.0));
        }
        if let Ok(bytes) = std::fs::read(file) {
            let parsed = parser::parse_log(&String::from_utf8_lossy(&bytes).as_ref().into());
            corrupt += parsed.corrupt_lines;
            for error in parsed.errors.into_iter().chain(parsed.warnings) {
                on_log(error);
            }
            if let Some(session) = parsed.session {
                result.push(data::session_value(&session)?);
            }
        }
        on_progress(progress(
            files.len(),
            index + 1,
            started.elapsed().as_secs_f64() * 1000.0,
        ));
    }
    if corrupt > 0 {
        on_log(format!("pi-atlas: skipped {corrupt} corrupt JSONL line(s)").into());
    }
    result.sort_by(|left, right| {
        compare_text(left, right, "timestamp").then_with(|| compare_text(left, right, "sessionId"))
    });
    write_cache(cache, &compute_signature(sessions)?, &result, now)?;
    Ok(result)
}

fn compare_text(left: &JsValue, right: &JsValue, key: &str) -> std::cmp::Ordering {
    pi_js_compat::collator::locale_compare_utf16(data::text(left, key).units(), data::text(right, key).units())
}

fn progress(total: usize, done: usize, elapsed: f64) -> LoadingProgress {
    LoadingProgress {
        total,
        done,
        pct: if total > 0 {
            math_round(done as f64 / total as f64 * 100.0)
        } else {
            0.0
        },
        remaining_time_ms: (done >= 3).then(|| math_round(elapsed / done as f64 * (total - done) as f64)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn loads_nested_logs_and_invalidates_cached_aggregates() -> Result<(), JsString> {
        let dir = tempfile::tempdir().map_err(|error| JsString::from(error.to_string()))?;
        let sessions = dir.path().join("sessions");
        std::fs::create_dir_all(sessions.join("nested")).map_err(|error| JsString::from(error.to_string()))?;
        let log = sessions.join("nested/a.jsonl");
        std::fs::write(
            &log,
            r#"{"type":"session","id":"first","timestamp":"2026-10-25T10:00:00Z"}"#,
        )
        .map_err(|error| JsString::from(error.to_string()))?;
        let cache = dir.path().join("cache.json");
        let loaded = load_aggregate(&cache, &sessions, 0.0)?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["sessionId"].as_string(), Some(&JsString::from("first")));
        std::fs::write(
            &log,
            r#"{"type":"session","id":"second-longer","timestamp":"2026-10-25T10:00:00Z"}"#,
        )
        .map_err(|error| JsString::from(error.to_string()))?;
        let reloaded = load_aggregate(&cache, &sessions, 0.0)?;
        assert_eq!(
            reloaded[0]["sessionId"].as_string(),
            Some(&JsString::from("second-longer"))
        );
        assert_eq!(read_cache(&cache).expect("cache")["sessions"], JsValue::Array(reloaded));
        Ok(())
    }

    #[test]
    fn exchanges_cache_payload_with_frozen_session_values() -> Result<(), JsString> {
        let dir = tempfile::tempdir().map_err(|error| JsString::from(error.to_string()))?;
        let path = dir.path().join("pi-atlas-cache.json");
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        let sessions: Vec<_> = fixture["logs"]
            .as_array()
            .expect("logs")
            .iter()
            .map(|log| log["expected"].clone())
            .filter(|value| !value.is_null())
            .collect();
        let now = pi_js_compat::date::date_parse("2026-10-25T12:00:00Z");
        write_cache(&path, &"abc".into(), &sessions, now)?;
        let payload = read_cache(&path).ok_or_else(|| JsString::from("cache missing"))?;
        assert_eq!(payload["sessions"], JsValue::Array(sessions));
        assert_eq!(payload["version"].as_string(), Some(&JsString::from("0.2.1")));
        assert_eq!(
            payload["generatedAt"].as_string(),
            Some(&JsString::from("2026-10-25T12:00:00.000Z"))
        );
        assert_eq!(payload["signature"].as_string(), Some(&JsString::from("abc")));
        let bytes = std::fs::read_to_string(&path).map_err(|error| JsString::from(error.to_string()))?;
        assert_eq!(bytes, js_json::stringify_lossless(&payload));
        std::fs::write(&path, "{\"signature\":\"\",\"sessions\":[]}")
            .map_err(|error| JsString::from(error.to_string()))?;
        assert!(read_cache(&path).is_none());
        Ok(())
    }
}
