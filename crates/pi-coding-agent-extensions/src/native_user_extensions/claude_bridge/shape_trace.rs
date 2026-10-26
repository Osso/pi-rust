//! Private content-free block-shape trace, source shape-trace.ts.
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(super) type SharedShapeTrace = Arc<Mutex<ShapeTrace>>;
#[derive(Clone)]
pub(super) struct ShapeSnapshot {
    trace: SharedShapeTrace,
    output: Arc<pi_ai::types::AssistantMessage>,
    message_id: JsString,
}
impl ShapeSnapshot {
    pub fn new(trace: &SharedShapeTrace, output: &pi_ai::types::AssistantMessage, message_id: &JsString) -> Self {
        Self {
            trace: trace.clone(),
            output: Arc::new(output.clone()),
            message_id: message_id.clone(),
        }
    }
    pub fn record_reset(&self) -> Result<(), CallbackError> {
        let content =
            pi_js_compat::js_json::to_value(&self.output.content).map_err(|e| CallbackError::new(e.to_string()))?;
        self.trace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record_pi_with_id("turn_reset", &content, &self.message_id)
    }
}

pub(super) struct ShapeTrace {
    path: PathBuf,
    max_bytes: u64,
    session: Option<JsString>,
    trace_id: String,
    capped: bool,
    message_id: Option<JsString>,
    block_types: Vec<JsValue>,
    tool_ids: Vec<JsValue>,
}
impl ShapeTrace {
    pub fn new(path: PathBuf, max_bytes: u64, session: Option<&JsString>) -> Result<Self, CallbackError> {
        Ok(Self {
            path,
            max_bytes,
            session: session.filter(|s| valid_session(s)).cloned(),
            trace_id: super::session_records::new_id()?,
            capped: false,
            message_id: None,
            block_types: Vec::new(),
            tool_ids: Vec::new(),
        })
    }
    pub fn record_sdk(&mut self, frame: &JsValue) -> Result<(), CallbackError> {
        if super::stream::string(frame, "type") == "assistant" {
            let assistant = frame.get("message").unwrap_or(&JsValue::Null);
            let (blocks, tools) = project(assistant.get("content").unwrap_or(&JsValue::Null), "tool_use");
            return self.append("sdk", "assistant", valid_id(assistant.get("id"), "msg_"), blocks, tools);
        }
        if super::stream::string(frame, "type") != "stream_event" {
            return Ok(());
        }
        let event = frame.get("event").unwrap_or(&JsValue::Null);
        let phase = super::stream::string(event, "type");
        if phase == "message_start" {
            self.message_id = valid_id(event.get("message").and_then(|v| v.get("id")), "msg_");
            self.block_types.clear();
            self.tool_ids.clear();
        } else if phase == "content_block_start" {
            let content = JsValue::Array(event.get("content_block").cloned().into_iter().collect());
            let (blocks, tools) = project(&content, "tool_use");
            self.block_types.extend(blocks);
            self.tool_ids.extend(tools);
        } else if phase != "message_stop" {
            return Ok(());
        }
        self.append(
            "sdk",
            &phase.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?,
            self.message_id.clone(),
            self.block_types.clone(),
            self.tool_ids.clone(),
        )
    }
    pub fn record_discarded(&mut self, content: &JsValue, message_id: &JsString) -> Result<(), CallbackError> {
        let (blocks, tools) = project(content, "toolCall");
        self.append(
            "pi",
            "discarded",
            valid_id(Some(&JsValue::String(message_id.clone())), "msg_"),
            blocks,
            tools,
        )
    }
    pub fn record_pi_with_id(
        &mut self,
        phase: &str,
        content: &JsValue,
        message_id: &JsString,
    ) -> Result<(), CallbackError> {
        let (blocks, tools) = project(content.get("content").unwrap_or(content), "toolCall");
        self.append(
            "pi",
            phase,
            valid_id(Some(&JsValue::String(message_id.clone())), "msg_"),
            blocks,
            tools,
        )
    }
    pub fn record_pi(&mut self, phase: &str, content: &JsValue) -> Result<(), CallbackError> {
        let (blocks, tools) = project(content.get("content").unwrap_or(content), "toolCall");
        self.append("pi", phase, self.message_id.clone(), blocks, tools)
    }
    fn append(
        &mut self,
        source: &str,
        phase: &str,
        message: Option<JsString>,
        blocks: Vec<JsValue>,
        tools: Vec<JsValue>,
    ) -> Result<(), CallbackError> {
        if self.capped {
            return Ok(());
        }
        let mut fields = pi_js_compat::js_json::JsObject::new();
        fields.insert("timestamp".into(), JsValue::String(super::logging::timestamp()?.into()));
        fields.insert("pid".into(), JsValue::Number(std::process::id().into()));
        fields.insert("traceId".into(), JsValue::String(self.trace_id.as_str().into()));
        if let Some(session) = &self.session {
            fields.insert("piSessionId".into(), JsValue::String(session.clone()));
        }
        fields.insert("source".into(), JsValue::String(source.into()));
        fields.insert("phase".into(), JsValue::String(phase.into()));
        if let Some(message) = message {
            fields.insert("sdkMessageId".into(), JsValue::String(message));
        }
        fields.insert("blockTypes".into(), JsValue::Array(blocks));
        fields.insert(
            if source == "sdk" { "toolIds" } else { "toolCallIds" }.into(),
            JsValue::Array(tools),
        );
        let text = pi_js_compat::js_json::stringify_lossless(&JsValue::Object(fields)) + "\n";
        let write = || -> std::io::Result<bool> {
            use std::io::Write;
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
            let parent = self
                .path
                .parent()
                .ok_or_else(|| std::io::Error::other("Missing shape trace parent"))?;
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(&self.path)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            if file.metadata()?.len() + text.len() as u64 > self.max_bytes {
                return Ok(false);
            }
            file.write_all(text.as_bytes())?;
            Ok(true)
        };
        match write() {
            Ok(false) => {
                self.capped = true;
                eprintln!("[claude-bridge shapes] file size cap reached");
            }
            Ok(true) => {}
            Err(error) => eprintln!(
                "[claude-bridge shapes] I/O {}",
                pi_js_compat::node_fs::node_error_code(&error)
            ),
        }
        Ok(())
    }
}
fn valid_session(text: &JsString) -> bool {
    let units = text.units();
    units.len() == 36
        && units.iter().enumerate().all(|(i, u)| {
            if [8, 13, 18, 23].contains(&i) {
                *u == 45
            } else {
                *u <= 127 && (*u as u8).is_ascii_hexdigit()
            }
        })
}
fn valid_id(value: Option<&JsValue>, prefix: &str) -> Option<JsString> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| {
            text.len() <= 128
                && text.len() > prefix.len()
                && text.starts_with(prefix)
                && text
                    .units()
                    .iter()
                    .all(|u| *u <= 127 && ((*u as u8).is_ascii_alphanumeric() || *u == 95 || *u == 45))
        })
        .cloned()
}
fn project(content: &JsValue, tool_type: &str) -> (Vec<JsValue>, Vec<JsValue>) {
    let mut blocks = Vec::new();
    let mut tools = Vec::new();
    for block in content.as_array().into_iter().flatten() {
        let kind = super::stream::string(block, "type");
        let known = ["thinking", "text", "tool_use", "redacted_thinking", "toolCall"]
            .iter()
            .any(|k| kind == *k);
        blocks.push(JsValue::String(if known { kind.clone() } else { "other".into() }));
        if kind == tool_type
            && let Some(id) = valid_id(block.get("id"), "toolu_")
        {
            tools.push(JsValue::String(id));
        }
    }
    (blocks, tools)
}
#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    use std::os::unix::fs::MetadataExt;
    #[test]
    fn claude_bridge_shape_trace_is_private_content_free_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shapes.jsonl");
        let mut trace =
            ShapeTrace::new(path.clone(), 4096, Some(&"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into())).unwrap();
        for frame in [
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_valid"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_valid","name":"secret-name","input":{"password":"secret-content"}}}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ] {
            trace.record_sdk(&js_json::parse_lossless(frame).unwrap()).unwrap();
        }
        trace
            .record_pi(
                "pi_final",
                &js_json::parse_lossless(
                    r#"[{"type":"toolCall","id":"toolu_valid","arguments":{"secret":"never log"}}]"#,
                )
                .unwrap(),
            )
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("password"));
        let records = text
            .lines()
            .map(|s| js_json::parse_lossless(s).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 4);
        let module = dir.path().join("shape-trace.mjs");
        std::fs::write(&module, include_str!("fixtures/shape-trace.mjs")).unwrap();
        let script = r#"
const fs=require('fs'),vm=require('vm');(async()=>{const m=new vm.SourceTextModule(fs.readFileSync(process.argv[1],'utf8'));await m.link(spec=>{const values=spec==='@earendil-works/pi-coding-agent'?{getAgentDir:()=>process.argv[2]}:require(spec);return new vm.SyntheticModule(Object.keys(values),function(){for(const[k,v]of Object.entries(values))this.setExport(k,v);});});await m.evaluate();const t=new m.namespace.ShapeTrace('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');for(const event of [{type:'message_start',message:{id:'msg_valid'}},{type:'content_block_start',content_block:{type:'tool_use',id:'toolu_valid',name:'secret-name',input:{password:'secret-content'}}},{type:'message_stop'}])t.recordSdk({type:'stream_event',event});t.recordPi('pi_final',[{type:'toolCall',id:'toolu_valid',arguments:{secret:'never log'}}],'msg_valid');process.stdout.write(fs.readFileSync(process.argv[2]+'/claude-bridge-shapes.jsonl','utf8'));})().catch(e=>{console.error(e);process.exitCode=1;});"#;
        let output = std::process::Command::new("node")
            .args(["--experimental-vm-modules", "-e", script])
            .arg(module)
            .arg(dir.path().join("oracle"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
        let frozen = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| js_json::parse_lossless(line).unwrap())
            .collect::<Vec<_>>();
        let normalize = |mut record: JsValue| {
            if let JsValue::Object(fields) = &mut record {
                for field in ["timestamp", "pid", "traceId"] {
                    fields.shift_remove(field);
                }
            }
            record
        };
        assert_eq!(
            records.iter().cloned().map(normalize).collect::<Vec<_>>(),
            frozen.into_iter().map(normalize).collect::<Vec<_>>()
        );
        assert_eq!(
            records[1].get("toolIds").unwrap(),
            &js_json::parse_lossless(r#"["toolu_valid"]"#).unwrap()
        );
        assert_eq!(
            records[3].get("toolCallIds").unwrap(),
            &js_json::parse_lossless(r#"["toolu_valid"]"#).unwrap()
        );
        assert_eq!(
            records[3].get("blockTypes").unwrap(),
            &js_json::parse_lossless(r#"["toolCall"]"#).unwrap()
        );
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let mut capped = ShapeTrace::new(path.clone(), text.len() as u64, None).unwrap();
        capped.record_pi("pi_final", &JsValue::Array(Vec::new())).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
}
