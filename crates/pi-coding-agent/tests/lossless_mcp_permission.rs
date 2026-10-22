use pi_coding_agent::core::extensions::types::{ToolCallEvent, ToolName};
use pi_coding_agent::core::permissions::mcp_permission_prompt::{
    PermissionPromptHandlerOptions, create_permission_prompt_handler,
};
use pi_js_compat::js_json::{JsObject, JsString};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[tokio::test]
async fn mcp_permission_request_retains_native_cwd() {
    let cwd = JsString::from_utf16(vec![0xd800]);
    let expected = cwd.clone();
    let called = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&called);
    let handler = create_permission_prompt_handler(PermissionPromptHandlerOptions {
        permission_prompt_tool: Some("mcp__fixture__approval".into()),
        cwd,
        call_tool: Arc::new(move |_, request| {
            assert_eq!(request.cwd, expected);
            observed.store(true, Ordering::SeqCst);
            Box::pin(async { Ok(pi_js_compat::js_json!({"behavior":"allow"})) })
        }),
        desktop_notifier: None,
        rule_store: None,
    });
    let mut event = ToolCallEvent {
        tool_name: ToolName::Custom("fixture".into()),
        tool_call_id: "call".into(),
        bypass_permissions: false,
        input: JsObject::new(),
    };
    assert!(handler(&mut event).await.is_none());
    assert!(called.load(Ordering::SeqCst));
}
