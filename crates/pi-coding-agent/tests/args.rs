use pi_agent_core::harness::ThinkingLevel;
use pi_coding_agent::cli::args::{
    ApprovalPresetName, DiagnosticType, ExtensionFlag, ListModels, Mode, UnknownFlagValue, parse_args,
    parse_thinking_level, print_help,
};

fn parse(values: &[&str]) -> pi_coding_agent::cli::args::Args {
    parse_args(&values.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
}

fn messages(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn parses_thinking_levels() {
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        assert_eq!(
            parse(&["--thinking", level]).thinking,
            Some(parse_thinking_level(level).unwrap())
        );
    }
}

#[test]
fn rejects_invalid_thinking_level_with_warning() {
    let result = parse(&["--thinking", "bogus"]);
    assert_eq!(result.thinking, None);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].kind, DiagnosticType::Warning);
    assert_eq!(
        result.diagnostics[0].message,
        "Invalid thinking level \"bogus\". Valid values: off, minimal, low, medium, high, xhigh, max, ultra"
    );
}

#[test]
fn parses_tools_and_extensions_as_metadata_commands() {
    let tools = parse(&["tools"]);
    assert_eq!(tools.list_tools, Some(true));
    assert!(tools.messages.is_empty());
    let extensions = parse(&["extensions"]);
    assert_eq!(extensions.list_extensions, Some(true));
    assert!(extensions.messages.is_empty());
}

#[test]
fn parses_login_provider_and_missing_provider() {
    let login = parse(&["login", "openai-codex-gc"]);
    assert_eq!(login.login_provider.as_deref(), Some("openai-codex-gc"));
    assert!(login.messages.is_empty());

    let missing = parse(&["login"]);
    assert_eq!(missing.login_provider, None);
    assert_eq!(
        missing.diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "login requires a provider".to_owned(),
        }]
    );
}

#[test]
fn parses_help_and_version_flags() {
    assert_eq!(parse(&["--help"]).help, Some(true));
    assert_eq!(parse(&["-h"]).help, Some(true));
    assert_eq!(parse(&["--version"]).version, Some(true));
    assert_eq!(parse(&["-v"]).version, Some(true));
}

#[test]
fn parses_print_flag_and_prompt_consumption() {
    assert_eq!(parse(&["--print"]).print, Some(true));
    assert_eq!(parse(&["-p"]).print, Some(true));

    let prompt = "---\ntitle: hello\n---\nSay hi.";
    let result = parse(&["-p", prompt]);
    assert_eq!(result.print, Some(true));
    assert_eq!(result.messages, vec![prompt.to_owned()]);
    assert!(result.unknown_flags.is_empty());

    let result = parse(&["-p", "--provider", "openai", "Say hi."]);
    assert_eq!(result.print, Some(true));
    assert_eq!(result.provider.as_deref(), Some("openai"));
    assert_eq!(result.messages, vec!["Say hi.".to_owned()]);

    let triple_dash = parse(&["-p", "---x"]);
    assert_eq!(triple_dash.messages, vec!["---x".to_owned()]);

    let file = parse(&["-p", "@prompt.md"]);
    assert!(file.messages.is_empty());
    assert_eq!(file.file_args, vec!["prompt.md".to_owned()]);
}

#[test]
fn parses_continue_resume_worktree_flags() {
    assert_eq!(parse(&["--continue"]).continue_session, Some(true));
    assert_eq!(parse(&["-c"]).continue_session, Some(true));
    assert_eq!(parse(&["--resume"]).resume, Some(true));
    assert_eq!(parse(&["-r"]).resume, Some(true));
    assert_eq!(
        parse(&["--worktree", "feature-a"]).worktree.as_deref(),
        Some("feature-a")
    );
    assert_eq!(parse(&["-w", "feature-a"]).worktree.as_deref(), Some("feature-a"));
    assert_eq!(
        parse(&["--worktree"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--worktree requires a value".to_owned(),
        }]
    );
}

#[test]
fn parses_flags_with_values() {
    assert_eq!(parse(&["--provider", "openai"]).provider.as_deref(), Some("openai"));
    assert_eq!(parse(&["--model", "gpt-4o"]).model.as_deref(), Some("gpt-4o"));
    assert_eq!(
        parse(&["--api-key", "sk-test-key"]).api_key.as_deref(),
        Some("sk-test-key")
    );
    assert_eq!(
        parse(&["--system-prompt", "You are a helpful assistant"])
            .system_prompt
            .as_deref(),
        Some("You are a helpful assistant")
    );
    assert_eq!(
        parse(&["--append-system-prompt", "Additional context"]).append_system_prompt,
        Some(vec!["Additional context".to_owned()])
    );
    assert_eq!(
        parse(&[
            "--append-system-prompt",
            "Context A",
            "--append-system-prompt",
            "Context B"
        ])
        .append_system_prompt,
        Some(vec!["Context A".to_owned(), "Context B".to_owned()])
    );
    assert_eq!(parse(&["--mode", "json"]).mode, Some(Mode::Json));
    assert_eq!(parse(&["--mode", "rpc"]).mode, Some(Mode::Rpc));
    assert_eq!(parse(&["--mode", "text"]).mode, Some(Mode::Text));
    assert_eq!(parse(&["--mode", "bogus"]).mode, None);
    assert_eq!(
        parse(&["--session", "/path/to/session.jsonl"]).session,
        Some("/path/to/session.jsonl".into())
    );
    assert_eq!(
        parse(&["--session-id", "orchestrated-session"]).session_id.as_deref(),
        Some("orchestrated-session")
    );
    let fork = parse(&["--fork", "1234abcd"]);
    assert_eq!(fork.fork.as_deref(), Some("1234abcd"));
    assert!(fork.messages.is_empty());
    assert_eq!(
        parse(&["--export", "session.jsonl"]).export.as_deref(),
        Some("session.jsonl")
    );
    assert_eq!(
        parse(&["--models", "gpt-4o,claude-sonnet,gemini-pro"]).models,
        Some(messages(&["gpt-4o", "claude-sonnet", "gemini-pro"]))
    );
    assert_eq!(
        parse(&["--models", " gpt-4o , claude-sonnet "]).models,
        Some(messages(&["gpt-4o", "claude-sonnet"]))
    );
}

#[test]
fn parses_name_flag() {
    assert_eq!(parse(&["--name", "my-session"]).name.as_deref(), Some("my-session"));
    assert_eq!(parse(&["-n", "quick-session"]).name.as_deref(), Some("quick-session"));
    assert_eq!(parse(&["--name", ""]).name.as_deref(), Some(""));
    assert_eq!(
        parse(&["--name"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--name requires a value".to_owned(),
        }]
    );
    let combined = parse(&["--name", "named-run", "--print", "--model", "gpt-4o", "hello"]);
    assert_eq!(combined.name.as_deref(), Some("named-run"));
    assert_eq!(combined.print, Some(true));
    assert_eq!(combined.model.as_deref(), Some("gpt-4o"));
    assert_eq!(combined.messages, messages(&["hello"]));
}

#[test]
fn parses_session_and_extension_flags() {
    assert_eq!(parse(&["--no-session"]).no_session, Some(true));
    assert_eq!(
        parse(&["--extension", "./my-extension.ts"]).extensions,
        Some(messages(&["./my-extension.ts"]))
    );
    assert_eq!(
        parse(&["-e", "./my-extension.ts"]).extensions,
        Some(messages(&["./my-extension.ts"]))
    );
    assert_eq!(
        parse(&["--extension", "./ext1.ts", "-e", "./ext2.ts"]).extensions,
        Some(messages(&["./ext1.ts", "./ext2.ts"]))
    );
    assert_eq!(parse(&["--no-extensions"]).no_extensions, Some(true));
    let combined = parse(&["--no-extensions", "-e", "foo.ts", "-e", "bar.ts"]);
    assert_eq!(combined.no_extensions, Some(true));
    assert_eq!(combined.extensions, Some(messages(&["foo.ts", "bar.ts"])));
}

#[test]
fn parses_resource_flags() {
    assert_eq!(
        parse(&["--skill", "./skill-dir"]).skills,
        Some(messages(&["./skill-dir"]))
    );
    assert_eq!(
        parse(&["--skill", "./skill-a", "--skill", "./skill-b"]).skills,
        Some(messages(&["./skill-a", "./skill-b"]))
    );
    assert_eq!(
        parse(&["--prompt-template", "./prompts"]).prompt_templates,
        Some(messages(&["./prompts"]))
    );
    assert_eq!(
        parse(&["--prompt-template", "./one", "--prompt-template", "./two"]).prompt_templates,
        Some(messages(&["./one", "./two"]))
    );
    assert_eq!(
        parse(&["--theme", "./theme.json"]).themes,
        Some(messages(&["./theme.json"]))
    );
    assert_eq!(
        parse(&["--theme", "./dark.json", "--theme", "./light.json"]).themes,
        Some(messages(&["./dark.json", "./light.json"]))
    );
    assert_eq!(parse(&["--no-skills"]).no_skills, Some(true));
    assert_eq!(parse(&["--no-prompt-templates"]).no_prompt_templates, Some(true));
    assert_eq!(parse(&["--no-themes"]).no_themes, Some(true));
    assert_eq!(parse(&["--no-context-files"]).no_context_files, Some(true));
    assert_eq!(parse(&["-nc"]).no_context_files, Some(true));
}

#[test]
fn parses_project_approval_and_offline_flags() {
    assert_eq!(parse(&["--approve"]).project_trust_override, Some(true));
    assert_eq!(parse(&["-a"]).project_trust_override, Some(true));
    assert_eq!(parse(&["--no-approve"]).project_trust_override, Some(false));
    assert_eq!(parse(&["-na"]).project_trust_override, Some(false));
    assert_eq!(parse(&["--verbose"]).verbose, Some(true));
    assert_eq!(parse(&["--offline"]).offline, Some(true));
}

#[test]
fn parses_approval_presets_and_policies() {
    assert_eq!(
        parse(&["--approval-preset", "llm-approved-deny"]).approval_preset,
        Some(ApprovalPresetName::LlmApprovedDeny)
    );
    assert_eq!(
        parse(&["--approval-preset", "llm-approved-ask"]).approval_preset,
        Some(ApprovalPresetName::LlmApprovedAsk)
    );
    assert_eq!(
        parse(&["--approval-preset", "never-ask-deny"]).approval_preset,
        Some(ApprovalPresetName::NeverAskDeny)
    );
    assert_eq!(
        parse(&["--approval-preset", "auto-approve"]).approval_preset,
        Some(ApprovalPresetName::AutoApprove)
    );
    assert!(
        !parse(&["--approval-preset", "auto-approve"])
            .unknown_flags
            .contains_key("approval-preset")
    );

    assert_eq!(
        parse(&["--approval-policy", "never"]).approval_preset,
        Some(ApprovalPresetName::NeverAskDeny)
    );
    assert_eq!(
        parse(&["--approval-policy", "auto-approve"]).approval_preset,
        Some(ApprovalPresetName::AutoApprove)
    );
    assert_eq!(
        parse(&["--dangerously-bypass-approvals"]).approval_preset,
        Some(ApprovalPresetName::AutoApprove)
    );

    assert_eq!(
        parse(&["--approval-preset", "bogus"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "Invalid --approval-preset value: bogus".to_owned(),
        }]
    );
    assert_eq!(
        parse(&["--approval-preset"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--approval-preset requires a value".to_owned(),
        }]
    );
    assert_eq!(
        parse(&["--approval-preset", "--print"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--approval-preset requires a value".to_owned(),
        }]
    );
    assert_eq!(
        parse(&["--approval-policy", "bogus"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "Invalid --approval-policy value: bogus".to_owned(),
        }]
    );
    assert_eq!(
        parse(&["--approval-policy"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--approval-policy requires a value".to_owned(),
        }]
    );
}

#[test]
fn removed_tui_mode_uses_unknown_flag_contract() {
    let parsed = parse(&["--tui-mode", "fullscreen"]);
    assert_eq!(
        parsed.unknown_flags.get("tui-mode"),
        Some(&UnknownFlagValue::String("fullscreen".to_owned()))
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn parses_tool_flags() {
    assert_eq!(parse(&["--no-tools"]).no_tools, Some(true));
    assert_eq!(parse(&["-nt"]).no_tools, Some(true));
    assert_eq!(parse(&["--no-builtin-tools"]).no_builtin_tools, Some(true));
    assert_eq!(parse(&["-nbt"]).no_builtin_tools, Some(true));
    assert_eq!(
        parse(&["--tools", "read,bash"]).tools,
        Some(messages(&["read", "bash"]))
    );
    assert_eq!(parse(&["-t", "read,bash"]).tools, Some(messages(&["read", "bash"])));
    assert_eq!(
        parse(&["--exclude-tools", "read,bash"]).exclude_tools,
        Some(messages(&["read", "bash"]))
    );
    assert_eq!(
        parse(&["-xt", "read,bash"]).exclude_tools,
        Some(messages(&["read", "bash"]))
    );
    assert_eq!(
        parse(&["--tools", " read , ,bash "]).tools,
        Some(messages(&["read", "bash"]))
    );
    let permission = parse(&["--permission-prompt-tool", "mcp__approval__prompt"]);
    assert_eq!(
        permission.permission_prompt_tool.as_deref(),
        Some("mcp__approval__prompt")
    );
    assert!(permission.messages.is_empty());
    assert!(permission.unknown_flags.is_empty());
    assert_eq!(
        parse(&["--permission-prompt-tool"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "--permission-prompt-tool requires a value".to_owned(),
        }]
    );
    let combined = parse(&["--no-tools", "--tools", "read,bash"]);
    assert_eq!(combined.no_tools, Some(true));
    assert_eq!(combined.tools, Some(messages(&["read", "bash"])));
    let combined = parse(&["--no-builtin-tools", "--tools", "read,bash"]);
    assert_eq!(combined.no_builtin_tools, Some(true));
    assert_eq!(combined.tools, Some(messages(&["read", "bash"])));
}

#[test]
fn parses_messages_files_and_unknown_flags() {
    assert_eq!(parse(&["hello", "world"]).messages, messages(&["hello", "world"]));
    assert_eq!(
        parse(&["@README.md", "@src/main.ts"]).file_args,
        messages(&["README.md", "src/main.ts"])
    );
    let mixed = parse(&["@file.txt", "explain this", "@image.png"]);
    assert_eq!(mixed.file_args, messages(&["file.txt", "image.png"]));
    assert_eq!(mixed.messages, messages(&["explain this"]));

    let unknown = parse(&["--unknown-flag", "message"]);
    assert!(unknown.messages.is_empty());
    assert_eq!(
        unknown.unknown_flags.get("unknown-flag"),
        Some(&UnknownFlagValue::String("message".to_owned()))
    );

    let boolean = parse(&["--unknown-flag"]);
    assert_eq!(
        boolean.unknown_flags.get("unknown-flag"),
        Some(&UnknownFlagValue::Boolean(true))
    );

    let equals = parse(&["--unknown-flag=value"]);
    assert_eq!(
        equals.unknown_flags.get("unknown-flag"),
        Some(&UnknownFlagValue::String("value".to_owned()))
    );

    let at_value = parse(&["--unknown-flag", "@file"]);
    assert_eq!(
        at_value.unknown_flags.get("unknown-flag"),
        Some(&UnknownFlagValue::Boolean(true))
    );
    assert_eq!(at_value.file_args, messages(&["file"]));

    assert_eq!(
        parse(&["-z"]).diagnostics,
        vec![pi_coding_agent::cli::args::Diagnostic {
            kind: DiagnosticType::Error,
            message: "Unknown option: -z".to_owned(),
        }]
    );
}

#[test]
fn parses_list_models_with_optional_search() {
    assert_eq!(parse(&["--list-models"]).list_models, Some(ListModels::All));
    assert_eq!(
        parse(&["--list-models", "sonnet"]).list_models,
        Some(ListModels::Search("sonnet".to_owned()))
    );
    assert_eq!(parse(&["--list-models", "--print"]).list_models, Some(ListModels::All));
    assert_eq!(parse(&["--list-models", "@file"]).list_models, Some(ListModels::All));
}

#[test]
fn parses_complex_combinations() {
    let result = parse(&[
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet",
        "--print",
        "--thinking",
        "high",
        "@prompt.md",
        "Do the task",
    ]);
    assert_eq!(result.provider.as_deref(), Some("anthropic"));
    assert_eq!(result.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(result.print, Some(true));
    assert_eq!(result.thinking, Some(ThinkingLevel::High));
    assert_eq!(result.file_args, messages(&["prompt.md"]));
    assert_eq!(result.messages, messages(&["Do the task"]));
}

#[test]
fn prints_help_with_and_without_extension_flags() {
    let help = print_help(&[]);
    assert!(help.starts_with("pi - AI coding assistant with file, command, and structural code tools\n"));
    assert!(help.contains("\nUsage:\n  pi [options] [@files...] [messages...]\n"));
    assert!(help.contains("  --help, -h                     Show this help\n"));
    assert!(!help.contains("Extension CLI Flags:"));
    assert!(help.ends_with("  ls     - List directory contents (read-only, off by default)\n\n"));

    let with_flags = print_help(&[
        ExtensionFlag {
            name: "plan".to_owned(),
            description: Some("Enable plan mode".to_owned()),
            kind: pi_coding_agent::cli::args::ExtensionFlagType::Boolean,
            default: None,
            extension_path: "/ext/plan.ts".into(),
        },
        ExtensionFlag {
            name: "model-extra".to_owned(),
            description: None,
            kind: pi_coding_agent::cli::args::ExtensionFlagType::String,
            default: None,
            extension_path: "/ext/extra.ts".into(),
        },
    ]);
    assert!(with_flags.contains(
        "\nExtension CLI Flags:\n  --plan                      Enable plan mode\n  --model-extra <value>       Registered by /ext/extra.ts\n\n"
    ));
}

#[test]
fn help_keeps_extension_path_native_until_the_stdout_boundary() {
    let mut path = pi_js_compat::js_json::JsString::from("/ext/");
    path.push_str(pi_js_compat::js_json::JsString::from_utf16(vec![0xDC00]));
    path.push_str(".ts");
    let help = print_help(&[ExtensionFlag {
        name: "raw".to_owned(),
        description: None,
        kind: pi_coding_agent::cli::args::ExtensionFlagType::Boolean,
        default: None,
        extension_path: path,
    }]);
    assert!(help.contains("  --raw                       Registered by /ext/\u{FFFD}.ts\n"));
}
