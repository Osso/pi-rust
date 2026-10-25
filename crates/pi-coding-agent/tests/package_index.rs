//! Package-root export surface (`packages/coding-agent/src/index.ts`).
//!
//! Compile-time proof that the crate root exposes the source export names plus
//! a few runtime checks for representative values.

use pi_coding_agent::core::extensions::Handler;
use pi_coding_agent::{
    AgentSession, AgentSessionEvent, AgentSessionRuntime, Args, AskArchitectToolDetails, AskArchitectToolInput,
    AuthStorage, BashToolOptions, CompactOptions, CompactionSettings, ContextUsage, DEFAULT_MAX_BYTES,
    DEFAULT_MAX_LINES, DefaultPackageManager, DefaultResourceLoader, EditDiffResult, ExtensionApi, ExtensionContext,
    ExtensionRunner, ExtensionUiContext, ExtensionUiDialogOptions, FileOperations, InteractiveMode, ModelRegistry,
    PromptOptions, ProviderConfig, RegisteredCommand, RegisteredTool, SessionEntry, SessionManager, SettingsManager,
    Skill, SkillFrontmatter, ToolCallEvent, ToolDefinition, ToolExecutionMode, TruncationOptions, WidgetPlacement,
    compact, config_dir_name, convert_to_llm, create_agent_session, create_ask_architect_tool_definition,
    create_coding_tools, create_edit_tool, create_event_bus, create_extension_runtime, create_read_only_tools,
    create_synthetic_source_info, discover_and_load_extensions, format_size, generate_unified_patch, get_agent_dir,
    get_markdown_theme, load_skills_from_dir, parse_args, parse_skill_block, render_diff,
    session_entry_to_context_messages, truncate_head, version,
};
use pi_js_compat::js_json::JsString;

#[test]
fn crate_root_exposes_frozen_auth_footer_and_event_types() {
    use pi_coding_agent::{
        ApiKeyCredential, AuthCredential, AuthStatus, CompactionReason, ExtensionStatus, ModelInfo,
        ModelRequestEndEvent, ModelRequestStartEvent, OAuthCredential,
    };
    let credential = AuthCredential::ApiKey(ApiKeyCredential {
        key: Some(JsString::from_utf16(vec![115, 101, 99, 114, 101, 116, 0xd800])),
        env: None,
    });
    let value = pi_js_compat::js_json::to_value(&credential).unwrap();
    let expected = pi_js_compat::js_json::parse_lossless(r#"{"type":"api_key","key":"secret\ud800"}"#).unwrap();
    assert_eq!(value, expected);
    let storage = AuthStorage::in_memory([("root-fixture", value.clone())]);
    assert_eq!(storage.get("root-fixture"), Some(value));
    let status: AuthStatus = storage.get_auth_status("root-fixture");
    assert!(status.configured);
    let raw = JsString::from_utf16(vec![0xd800]);
    assert_eq!(ExtensionStatus::Text(raw.clone()).evaluate(None).unwrap(), Some(raw));
    let oauth = OAuthCredential::new("refresh", "access", 123.0);
    assert_eq!(
        oauth.to_js_value().get("access").and_then(|value| value.as_string()),
        Some(&JsString::from("access"))
    );
    let _: Option<CompactionReason> = None;
    let _: Option<ModelInfo> = None;
    let _: Option<ModelRequestStartEvent> = None;
    let _: Option<ModelRequestEndEvent> = None;
}

fn assert_paths_exist() {
    // Type-only imports above are the proof; these function pointers keep the
    // value exports referenced so a rename breaks compilation.
    let _: fn(&[String]) -> Args = parse_args;
    let _: fn() -> &'static JsString = version;
    let _: fn() -> &'static JsString = config_dir_name;
    let _: fn() -> Result<JsString, pi_coding_agent::config::ConfigError> = get_agent_dir;
    let _: fn(f64) -> String = format_size;
    let _: fn(JsString, JsString, JsString, usize) -> JsString = generate_unified_patch;
    let _ = convert_to_llm;
    let _ = create_event_bus;
    let _ = create_extension_runtime;
    let _ = discover_and_load_extensions;
    let _ = create_synthetic_source_info;
    let _ = load_skills_from_dir;
    let _: fn(JsString) -> Option<_> = parse_skill_block;
    let _ = render_diff;
    let _ = session_entry_to_context_messages;
    let _: fn(JsString, _) -> _ = truncate_head;
    let _ = compact;
    let _ = create_agent_session;
    let _ = create_coding_tools;
    let _ = create_read_only_tools;
    let _: fn(JsString, _) -> _ = create_edit_tool;
    let _ = create_ask_architect_tool_definition;
    let _ = get_markdown_theme;
    let _: Option<fn()> = None::<fn()>;
    let _: Option<Box<dyn Handler<pi_coding_agent::AgentStartEvent, ()>>> = None;
}

#[test]
fn crate_root_exposes_the_package_index_surface() {
    assert_paths_exist();
    assert_eq!(format_size(512.0), "512B");
    assert_eq!(DEFAULT_MAX_LINES, 2000);
    assert_eq!(DEFAULT_MAX_BYTES, 10 * 1024);
    assert!(!version().is_empty());
    assert!(!config_dir_name().is_empty());
    let _: TruncationOptions = TruncationOptions::default();
    let _: Option<CompactionSettings> = None;
    let _: ToolExecutionMode = ToolExecutionMode::Parallel;
    let _: WidgetPlacement = WidgetPlacement::AboveEditor;
    let _: SkillFrontmatter = SkillFrontmatter::new();
    let _: Option<CompactOptions> = None;
    let _: Option<ContextUsage> = None;
    let _: Option<FileOperations> = None;
    let _: Option<PromptOptions> = None;
    let _: Option<EditDiffResult> = None;
    let _: Option<ToolDefinition> = None;
    let _: Option<RegisteredTool> = None;
    let _: Option<RegisteredCommand> = None;
    let _: Option<ProviderConfig> = None;
    let _: Option<ExtensionUiDialogOptions> = None;
    let _: Option<Box<dyn ExtensionUiContext>> = None;
    let _: Option<ExtensionApi> = None;
    let _: Option<ExtensionContext> = None;
    let _: Option<ExtensionRunner> = None;
    let _: Option<ToolCallEvent> = None;
    let _: Option<AgentSessionEvent> = None;
    let _: Option<AgentSession> = None;
    let _: Option<AgentSessionRuntime> = None;
    let _: Option<InteractiveMode> = None;
    let _: Option<AuthStorage> = None;
    let _: Option<ModelRegistry> = None;
    let _: Option<SessionManager> = None;
    let _: Option<SessionEntry> = None;
    let _: Option<SettingsManager> = None;
    let _: Option<DefaultResourceLoader> = None;
    let _: Option<DefaultPackageManager> = None;
    let _: Option<Skill> = None;
    let _: Option<BashToolOptions> = None;
    let _: Option<AskArchitectToolInput> = None;
    let _: Option<AskArchitectToolDetails> = None;
}
