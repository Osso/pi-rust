//! The cc-session-io record subset used by the bridge. JSON values remain lossless.
use std::path::{Path, PathBuf};

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};

use super::stream::string;

pub fn new_id() -> Result<String, CallbackError> {
    pi_ai::utils::random_uuid::random_uuid().map_err(|e| CallbackError::new(e.to_string()))
}

fn generate_slug() -> Result<String, CallbackError> {
    const ADJECTIVES: [&str; 32] = [
        "ancient", "bold", "bright", "calm", "clever", "cool", "crimson", "daring", "eager", "fast", "fierce",
        "gentle", "golden", "happy", "hidden", "iron", "keen", "lively", "mighty", "nimble", "pale", "proud", "quick",
        "rapid", "sharp", "silent", "smooth", "steady", "swift", "vivid", "warm", "wild",
    ];
    const NOUNS: [&str; 40] = [
        "arrow", "badge", "beacon", "blade", "brook", "castle", "cedar", "cloud", "comet", "crest", "dawn", "drift",
        "eagle", "ember", "falcon", "flame", "forge", "gale", "grove", "harbor", "hawk", "jade", "kettle", "lance",
        "maple", "marsh", "needle", "oak", "pearl", "pine", "ridge", "river", "rune", "sage", "shade", "spark",
        "stone", "thorn", "tower", "wave",
    ];
    let mut random = [0; 3];
    getrandom::fill(&mut random).map_err(|e| CallbackError::new(e.to_string()))?;
    while random[2] >= 240 {
        getrandom::fill(&mut random[2..]).map_err(|e| CallbackError::new(e.to_string()))?;
    }
    Ok(format!(
        "{}-{}-{}",
        ADJECTIVES[usize::from(random[0] % 32)],
        ADJECTIVES[usize::from(random[1] % 32)],
        NOUNS[usize::from(random[2] % 40)]
    ))
}

fn path_text(path: &Path) -> Result<JsString, CallbackError> {
    path.to_str()
        .map(JsString::from)
        .ok_or_else(|| CallbackError::new("Invalid Claude session path"))
}

pub fn project_path(cwd: &Path) -> Result<JsString, CallbackError> {
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    Ok(crate::core::tools::unicode::normalize_nfc(&path_text(&canonical)?))
}

pub fn session_path(root: &Path, cwd: &Path, id: &str) -> Result<PathBuf, CallbackError> {
    let project = project_path(cwd)?;
    let sanitized: String = project
        .units()
        .iter()
        .map(|u| {
            if *u <= 127 && (*u as u8).is_ascii_alphanumeric() {
                char::from(*u as u8)
            } else {
                '-'
            }
        })
        .collect();
    let hash = if sanitized.len() <= 200 {
        sanitized
    } else {
        let value = project
            .units()
            .iter()
            .fold(0_i32, |h, unit| h.wrapping_mul(31).wrapping_add(i32::from(*unit)));
        let mut n = value.unsigned_abs();
        let mut digits = Vec::new();
        loop {
            digits.push(char::from_digit(n % 36, 36).ok_or_else(|| CallbackError::new("Invalid project hash"))?);
            n /= 36;
            if n == 0 {
                break;
            }
        }
        format!("{}-{}", &sanitized[..200], digits.into_iter().rev().collect::<String>())
    };
    Ok(root.join("projects").join(hash).join(format!("{id}.jsonl")))
}

pub fn read_records(path: &Path) -> Result<Vec<JsValue>, CallbackError> {
    let content = match std::fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(CallbackError::new(pi_js_compat::node_fs::node_read_error(
                &error,
                &path_text(path)?,
            )));
        }
    };
    let mut records = Vec::new();
    for line in content
        .split(|b| *b == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
    {
        let Ok(line) = std::str::from_utf8(line) else {
            break;
        };
        let Ok(record) = js_json::parse_lossless(line) else {
            break;
        };
        records.push(record);
    }
    Ok(records)
}

pub fn write_records(path: &Path, records: &[JsValue]) -> Result<(), CallbackError> {
    let parent = path
        .parent()
        .ok_or_else(|| CallbackError::new("Claude session path has no parent"))?;
    std::fs::create_dir_all(parent).map_err(|e| CallbackError::new(e.to_string()))?;
    let content: String = records
        .iter()
        .map(|record| js_json::stringify_lossless(record) + "\n")
        .collect();
    std::fs::write(path, content)
        .map_err(|e| CallbackError::new(format!("Write Claude session {}: {e}", path.display())))
}

pub struct RecordWriter {
    pub records: Vec<JsValue>,
    session: JsString,
    cwd: JsString,
    model: JsString,
    parent: JsValue,
    timestamp: i64,
    slug: JsString,
}

impl RecordWriter {
    pub fn new(id: &str, cwd: &Path, model: &str, records: Vec<JsValue>) -> Result<Self, CallbackError> {
        let parent = records
            .iter()
            .rev()
            .find(|record| {
                ["user", "assistant", "attachment"]
                    .iter()
                    .any(|kind| string(record, "type") == *kind)
            })
            .and_then(|record| record.get("uuid"))
            .cloned()
            .unwrap_or(JsValue::Null);
        let slug = generate_slug()?;
        Ok(Self {
            records,
            session: id.into(),
            cwd: project_path(cwd)?,
            model: model.into(),
            parent,
            timestamp: pi_js_compat::date::js_now_millis() as i64,
            slug: slug.into(),
        })
    }

    fn base(&mut self, kind: &str) -> Result<indexmap::IndexMap<JsString, JsValue>, CallbackError> {
        let uuid = JsValue::String(new_id()?.into());
        let fields = JsValue::object([
            ("type", JsValue::String(kind.into())),
            ("uuid", uuid.clone()),
            ("parentUuid", self.parent.clone()),
            ("sessionId", JsValue::String(self.session.clone())),
            (
                "timestamp",
                JsValue::String(pi_js_compat::date::to_iso_string(self.timestamp).into()),
            ),
            ("isSidechain", JsValue::Bool(false)),
            ("cwd", JsValue::String(self.cwd.clone())),
            ("userType", JsValue::String("external".into())),
            ("version", JsValue::String("2.1.83".into())),
            ("gitBranch", JsValue::String("HEAD".into())),
            ("slug", JsValue::String(self.slug.clone())),
            ("entrypoint", JsValue::String("cli".into())),
        ]);
        self.parent = uuid;
        self.timestamp += 1;
        let JsValue::Object(fields) = fields else {
            return Err(CallbackError::new("Invalid record fields"));
        };
        Ok(fields)
    }

    pub fn attachment(&mut self, attachment: JsValue) -> Result<(), CallbackError> {
        let mut fields = self.base("attachment")?;
        fields.insert("attachment".into(), attachment);
        self.records.push(JsValue::Object(fields));
        Ok(())
    }

    fn user(&mut self, content: JsValue) -> Result<(), CallbackError> {
        let mut fields = self.base("user")?;
        fields.insert(
            "message".into(),
            JsValue::object([("role", JsValue::String("user".into())), ("content", content)]),
        );
        self.records.push(JsValue::Object(fields));
        Ok(())
    }

    fn assistant(&mut self, content: JsValue) -> Result<(), CallbackError> {
        let content = if content.as_string().is_some() {
            JsValue::Array(vec![JsValue::object([
                ("type", JsValue::String("text".into())),
                ("text", content),
            ])])
        } else {
            content
        };
        let has_tools = content
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|block| string(block, "type") == "tool_use"));
        let mut fields = self.base("assistant")?;
        fields.insert(
            "requestId".into(),
            JsValue::String(format!("req_syn_{}", &new_id()?.replace('-', "")[..24]).into()),
        );
        fields.insert(
            "message".into(),
            JsValue::object([
                (
                    "id",
                    JsValue::String(format!("msg_syn_{}", &new_id()?.replace('-', "")[..24]).into()),
                ),
                ("type", JsValue::String("message".into())),
                ("role", JsValue::String("assistant".into())),
                ("model", JsValue::String(self.model.clone())),
                ("content", content),
                (
                    "stop_reason",
                    JsValue::String(if has_tools { "tool_use" } else { "end_turn" }.into()),
                ),
                ("stop_sequence", JsValue::Null),
                (
                    "usage",
                    JsValue::object([
                        ("input_tokens", JsValue::Number(0.0)),
                        ("output_tokens", JsValue::Number(0.0)),
                    ]),
                ),
            ]),
        );
        self.records.push(JsValue::Object(fields));
        Ok(())
    }

    pub fn import_message(&mut self, message: &JsValue) -> Result<(), CallbackError> {
        let content = message.get("content").cloned().unwrap_or(JsValue::Null);
        if string(message, "role") == "assistant" {
            return self.assistant(content);
        }
        let Some(blocks) = content.as_array() else {
            return self.user(content);
        };
        let (mut results, rest): (Vec<_>, Vec<_>) = blocks
            .iter()
            .cloned()
            .partition(|block| string(block, "type") == "tool_result");
        for result in &mut results {
            if let JsValue::Object(fields) = result
                && fields.get("is_error").and_then(JsValue::as_bool) != Some(true)
            {
                fields.shift_remove("is_error");
            }
        }
        if !results.is_empty() {
            self.user(JsValue::Array(results))?;
        }
        if !rest.is_empty() {
            self.user(JsValue::Array(rest))?;
        }
        Ok(())
    }
}
