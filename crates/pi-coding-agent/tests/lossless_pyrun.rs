//! Lossless persisted Pyrun payloads; real bridge/runner types, no DB harness.
use pi_coding_agent::bundled_extensions::pyrun::detached_bridge::DetachedPyrunBridgeRequest;
use pi_coding_agent::bundled_extensions::pyrun::detached_runner::{
    DetachedPyrunRunnerInvocationOptions, get_detached_pyrun_runner_invocation, read_detached_pyrun_activation,
    write_detached_pyrun_activation,
};
use pi_coding_agent::core::session_control_db::DetachedJobOwnershipIdentity;
use pi_js_compat::js_json::encode_node_utf8;
use pi_js_compat::js_json::{self, JsString};

#[test]
fn persisted_runner_environment_is_an_object_with_raw_keys_and_values() {
    use pi_coding_agent::bundled_extensions::pyrun::runner::PyrunRunnerOptions;
    let expected = js_json::parse_lossless(r#"{"env":{"ENV_\ud800":"value-\udfff"}}"#).unwrap();
    let options: PyrunRunnerOptions = js_json::from_value(&expected).unwrap();
    let wire = js_json::to_value(&options).unwrap();
    assert!(wire["env"].is_object());
    assert_eq!(wire["env"], expected["env"]);
}

#[test]
fn canonical_console_approval_and_progress_metadata_keep_raw_units() {
    use pi_coding_agent::bundled_extensions::pyrun::runner::{CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate};
    let wire = js_json::parse_lossless(r#"{"type":"needs_approval","console":["text-\ud804",{"level":"level-\ud800","message":"message-\udfff"}],"approval":{"args":{"key-\ud800":1},"id":"id-\ud801","summary":"summary-\ud802","tool":"tool-\ud803"}}"#).unwrap();
    let result: CanonicalPyrunEvalResult = js_json::from_value(&wire).unwrap();
    assert_eq!(js_json::to_value(&result).unwrap(), wire);
    let update = CanonicalPyrunProgressUpdate {
        raw: js_json::parse_lossless(r#"{"type":"type-\ud800","method":"method-\ud801","status":"status-\ud802"}"#)
            .unwrap(),
    };
    assert_eq!(update.message_type(), update.raw["type"].as_string().unwrap().clone());
    assert_eq!(
        update.method().map(JsString::from),
        update.raw["method"].as_string().cloned()
    );
    assert_eq!(
        update.status().map(JsString::from),
        update.raw["status"].as_string().cloned()
    );
}

#[test]
fn activation_file_round_trips_raw_identity_at_raw_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = JsString::from(directory.path().to_str().unwrap()) + "/activation-" + JsString::from_utf16(vec![0xd800]);
    let value = js_json::parse_lossless(r#"{"jobId":"job-\ud800","owner":{"sessionId":"session-\udfff","agentId":null},"processIdentity":{"pid":1,"startTimeTicks":2},"outputLabel":"output-\ud801"}"#).unwrap();
    let identity: DetachedJobOwnershipIdentity = js_json::from_value(&value).unwrap();
    write_detached_pyrun_activation(&path, &identity).unwrap();
    assert!(std::path::Path::new(&encode_node_utf8(&path)).is_file());
    assert_eq!(read_detached_pyrun_activation(&path).unwrap(), Some(identity));
    let bytes = std::fs::read(encode_node_utf8(&path)).unwrap();
    assert_eq!(
        js_json::parse_lossless(String::from_utf8(bytes).unwrap()).unwrap(),
        value
    );
}

#[test]
fn invocation_keeps_raw_argv_until_spawn() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let manifest = JsString::from("/manifest-") + &raw;
    let executable = JsString::from("/exe-") + &raw;
    let entry = JsString::from("/entry-") + &raw + ".ts";
    let invocation = get_detached_pyrun_runner_invocation(
        &manifest,
        &DetachedPyrunRunnerInvocationOptions {
            compiled: false,
            entry_path: Some(entry.clone()),
            executable_path: Some(executable.clone()),
        },
    )
    .unwrap();
    assert_eq!(invocation.executable, executable);
    assert_eq!(
        invocation.args,
        vec![JsString::from("--experimental-strip-types"), entry, manifest]
    );
}

#[test]
fn persisted_bridge_request_retains_raw_method_ids_and_params() {
    let wire = r#"{"command":"request","identity":{"jobId":"job-\ud800","owner":{"sessionId":"session-\udfff","agentId":null},"processIdentity":{"pid":1,"startTimeTicks":2},"outputLabel":"output-\ud801"},"method":"unknown-\ud802","params":{"arg-\ud803":"value-\ud804"},"protocol":"pyrun-bridge-v1","replyTo":{"sessionId":"session-\udfff","agentId":"runner-\ud805"},"requestId":"request-\ud806","toolCallId":"tool-\ud807"}"#;
    let value = js_json::parse_lossless(wire).unwrap();
    let request: DetachedPyrunBridgeRequest = js_json::from_value(&value).unwrap();
    assert_eq!(js_json::to_value(&request).unwrap(), value);
    assert_eq!(
        request.method,
        JsString::from("unknown-") + JsString::from_utf16(vec![0xd802])
    );
}
