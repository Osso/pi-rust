//! Opt-in native `prompt-customizer.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::system_prompt::BuildSystemPromptOptions;

pub fn prompt_customizer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_before_agent_start(|event: BeforeAgentStartEvent, _: ExtensionContext| -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
        Box::pin(async move {
            let options=&event.system_prompt_options;
            let custom=add_tool_guidance(options,&event.system_prompt);
            let extra="\n## Extension-Added Context\n\nThis prompt includes tool guidance and skill information loaded dynamically.\nIf you have additional requirements, configure them via --append-system-prompt or project context files.\n";
            let append=match options.append_system_prompt.as_deref().filter(|s|!s.is_empty()) {
                Some(user)=>format!("{user}\n\n{extra}"), None=>extra.into()
            };
            Ok(Some(BeforeAgentStartEventResult {system_prompt:Some(format!("{custom}{append}")),message:None}))
        })
    }).map_err(HandlerError::new)
}

fn add_tool_guidance(options: &BuildSystemPromptOptions, base: &str) -> String {
    let has_tool = |name: &str| {
        options
            .selected_tools
            .as_ref()
            .is_some_and(|tools| tools.iter().any(|t| t == name))
    };
    let mut parts = Vec::new();
    if has_tool("read") {
        parts.extend([
            "• Use the `read` tool for file contents (supports text and images).".into(),
            "  - For large files, use `offset` and `limit` to read in chunks.".into(),
        ]);
    }
    if has_tool("bash") {
        parts.push(
            "• Execute commands with the `bash` tool. Use it for file operations like `ls`, `find`, `grep`.".into(),
        );
    }
    if has_tool("edit") {
        parts.push(
            "• Use the `edit` tool for precise text replacements in files. Match exact content including whitespace."
                .into(),
        );
    }
    if has_tool("write") {
        parts.push("• Use the `write` tool to create new files or overwrite existing ones completely.".into());
    }
    if let Some(skills) = options.skills.as_ref().filter(|skills| !skills.is_empty()) {
        let names = skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
        parts.extend([
            format!("\nAvailable skills: {names}"),
            "Use skill documentation for best practices on specific tools.".into(),
        ]);
    }
    if parts.is_empty() {
        base.into()
    } else {
        format!("{base}\n\n## Tool Guidance\n\n{}\n", parts.join("\n"))
    }
}
