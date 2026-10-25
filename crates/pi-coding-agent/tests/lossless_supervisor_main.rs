use pi_coding_agent::supervisor::main::{
    SupervisorConsolePromptQueue, create_supervisor_console_snapshot, validate_supervisor_extension_load,
};
use pi_js_compat::js_json::JsString;

#[test]
fn extension_failure_keeps_raw_paths_and_messages() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let path = JsString::from("/extension-") + &raw;
    let message = JsString::from("failure-") + &raw;
    assert_eq!(
        validate_supervisor_extension_load(&[(path.clone(), message.clone())]).unwrap_err(),
        JsString::from("Supervisor extension load failed: ") + path + ": " + message
    );
}

#[test]
fn console_queue_and_snapshot_keep_raw_identifiers_and_text() {
    let raw = JsString::from_utf16(vec![0xdfff]);
    let text = JsString::from("prompt-") + &raw;
    let id = JsString::from("id-") + &raw;
    let mut queue = SupervisorConsolePromptQueue::new();
    queue.enqueue(text.clone(), id.clone());
    let prompt = queue.take().unwrap();
    assert_eq!(prompt.text, text);
    assert_eq!(prompt.id, id);
    let cwd = JsString::from("/cwd-") + &raw;
    let snapshot = create_supervisor_console_snapshot(
        &cwd,
        1,
        pi_coding_agent::core::resident_console_transport::ResidentConsoleManagedBy::External,
        Some(&id),
        Vec::new(),
    );
    assert_eq!(snapshot.session_id, id);
    assert_eq!(snapshot.cwd, cwd);
}
