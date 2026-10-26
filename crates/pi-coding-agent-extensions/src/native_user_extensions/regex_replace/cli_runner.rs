//! JSON-file transport for the external regex-replace CLI.

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8, parse_lossless, stringify_lossless};

use crate::core::extensions::{ExtensionApi, ExtensionExecOptions, HandlerError};

#[derive(Clone)]
pub(super) struct JsonCliRunner {
    pub pi: ExtensionApi,
    pub binary_path: JsString,
}

impl JsonCliRunner {
    pub async fn run(&self, request: &JsValue, signal: Option<AbortSignal>) -> Result<JsValue, HandlerError> {
        let directory = tempfile::Builder::new()
            .prefix("regex-replace-")
            .tempdir()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let request_path = directory.path().join("request.json");
        let result = self.write_and_execute(request, signal, &request_path).await;
        directory
            .close()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        result
    }

    async fn write_and_execute(
        &self,
        request: &JsValue,
        signal: Option<AbortSignal>,
        request_path: &std::path::Path,
    ) -> Result<JsValue, HandlerError> {
        let serialized = stringify_lossless(request);
        tokio::fs::write(request_path, serialized.as_bytes())
            .await
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let request_arg = request_path
            .to_str()
            .ok_or_else(|| HandlerError::new("regex-replace request path is not valid UTF-8"))?
            .to_owned();
        let cwd = request.get("cwd").and_then(JsValue::as_string).cloned();
        let result = self
            .pi
            .exec(
                encode_node_utf8(&self.binary_path),
                vec![request_arg],
                Some(ExtensionExecOptions {
                    cwd,
                    signal,
                    timeout: None,
                }),
            )
            .await
            .map_err(HandlerError::new)?;
        if result.code != 0 {
            let message = result.stderr.trim();
            let message = if message.is_empty() {
                format!("regex-replace-json exited with code {}", result.code)
            } else {
                message.to_owned()
            };
            return Err(HandlerError::new(message));
        }
        let parsed = parse_lossless(&result.stdout).map_err(|error| HandlerError::new(error.into_message()))?;
        if !is_replace_result(&parsed) {
            return Err(HandlerError::new("regex-replace-json returned an invalid result"));
        }
        Ok(parsed)
    }
}

fn is_replace_result(value: &JsValue) -> bool {
    let has_boolean = value.get("dryRun").and_then(JsValue::as_bool).is_some();
    let has_hash = value.get("planHash").and_then(JsValue::as_string).is_some();
    let has_numbers = ["matchedFiles", "totalReplacements", "filesModified"]
        .iter()
        .all(|key| value.get(key).and_then(JsValue::as_f64).is_some());
    let has_changes = value.get("changes").and_then(JsValue::as_array).is_some();
    has_boolean && has_hash && has_numbers && has_changes
}
