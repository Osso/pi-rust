#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::cli::args::{
    Args, Diagnostic, DiagnosticType, ExtensionFlag, ExtensionFlagType, ListModels, Mode, UnknownFlagValue,
    is_valid_thinking_level, parse_args, print_help,
};
use serde_json::{Value, json};

#[test]
fn args_parsing_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");

    for case in parse_cases() {
        let expected = oracle.request(&json!({
            "id": case.id,
            "operation": "parse_args",
            "args": case.argv,
        }));
        let actual = response(case.id, parse_args(&case.argv));
        assert_eq!(expected, actual, "args mismatch: {}", case.name);
    }
}

#[test]
fn help_output_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");

    for (id, name, flags) in help_cases() {
        let expected = oracle.request(&json!({
            "id": id,
            "operation": "print_help",
            "extensionFlags": oracle_flags(&flags),
        }));
        let actual = json!({"id": id, "ok": true, "output": print_help(&flags)});
        assert_eq!(expected, actual, "help mismatch: {name}");
    }
}

#[test]
fn thinking_level_validation_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");
    let levels = [
        "off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra", "bogus", "", "HIGH", " high",
    ];
    let expected = oracle.request(&json!({
        "id": 28_200,
        "operation": "is_valid_thinking_level",
        "levels": levels,
    }));
    let actual = json!({
        "id": 28_200,
        "ok": true,
        "output": levels.iter().map(|level| is_valid_thinking_level(level)).collect::<Vec<_>>(),
    });
    assert_eq!(expected, actual);
}

#[test]
fn terminal_export_args_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");
    let cases: &[&[&str]] = &[
        &["--export-terminal", "session.jsonl", "--export-columns", "80"],
        &[
            "--export-terminal",
            "abc",
            "--export-columns",
            "001",
            "--offline",
            "--session-dir",
            "/tmp/sessions",
        ],
        &["--export-terminal"],
        &["--export-columns", "80"],
        &["--export-terminal", "--offline"],
        &["--export-terminal", ""],
        &["--export-terminal", "abc", "--export-columns", "-1"],
        &["--export-terminal", "abc", "--export-columns", "1.5"],
        &["--export-terminal", "abc", "--export-columns", "0"],
        &["--export-terminal", "abc", "--export-columns", "9007199254740992"],
        &["--export-terminal", "abc", "--export-columns", "--offline"],
        &["--export-terminal=abc", "--export-columns=80"],
        &["--export-terminal", "abc", "--export-columns", "80", "--help"],
        &["--export-terminal", "abc", "--export-columns", "80", "--no-approve"],
        &[
            "--export-terminal",
            "abc",
            "--export-columns",
            "80",
            "hello",
            "@file",
            "--unknown",
        ],
        &[
            "--export-terminal",
            "abc",
            "--export-columns",
            "80",
            "--no-supervisor",
            "--no-session",
        ],
    ];
    for (id, argv) in cases.iter().enumerate() {
        let argv = argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
        let expected = oracle.request(&json!({"id":id,"operation":"parse_args","args":argv}));
        assert_eq!(expected, response(id as u64, parse_args(&argv)), "{argv:?}");
    }
}

#[test]
fn every_builtin_flag_matches_frozen_with_ordinary_next_tokens() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");
    let flags = [
        "--help",
        "-h",
        "--version",
        "-v",
        "--mode",
        "--continue",
        "-c",
        "--resume",
        "-r",
        "--provider",
        "--model",
        "--api-key",
        "--system-prompt",
        "--append-system-prompt",
        "--name",
        "-n",
        "--no-session",
        "--no-supervisor",
        "--session",
        "--session-id",
        "--fork",
        "--session-dir",
        "--worktree",
        "-w",
        "--models",
        "--no-tools",
        "-nt",
        "--no-builtin-tools",
        "-nbt",
        "--tools",
        "-t",
        "--exclude-tools",
        "-xt",
        "--permission-prompt-tool",
        "--approval-preset",
        "--approval-policy",
        "--dangerously-bypass-approvals",
        "--thinking",
        "--print",
        "-p",
        "--export-terminal",
        "--export-columns",
        "--export",
        "--extension",
        "-e",
        "--no-extensions",
        "-ne",
        "--skill",
        "--prompt-template",
        "--theme",
        "--no-skills",
        "-ns",
        "--no-prompt-templates",
        "-np",
        "--no-themes",
        "--no-context-files",
        "-nc",
        "--list-models",
        "--refresh-models",
        "tools",
        "extensions",
        "login",
        "--verbose",
        "--approve",
        "-a",
        "--no-approve",
        "-na",
        "--offline",
    ];
    let next_tokens = [
        None,
        Some(""),
        Some("alpha"),
        Some("--help"),
        Some("@file"),
        Some("---prompt"),
        Some("001"),
    ];
    let mut id = 30_000;
    for flag in flags {
        for next in next_tokens {
            let argv: Vec<String> = std::iter::once(flag).chain(next).map(str::to_owned).collect();
            let expected = oracle.request(&json!({"id":id,"operation":"parse_args","args":argv}));
            assert_eq!(expected, response(id, parse_args(&argv)), "{argv:?}");
            id += 1;
        }
    }
}

#[test]
fn standalone_worker_diagnostics_and_repeated_fields_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("args-oracle.ts", "packages/coding-agent/src/cli/args.ts");
    let cases: &[&[&str]] = &[
        &["--no-supervisor"],
        &["--no-supervisor", "--no-session"],
        &[
            "--no-supervisor",
            "--no-session",
            "--session",
            "file",
            "--session-id",
            "id",
            "--session-dir",
            "dir",
            "--fork",
            "old",
            "-r",
            "-c",
            "-n",
            "named",
            "--export",
            "out",
        ],
        &[
            "--export",
            "out",
            "-n",
            "named",
            "-c",
            "-r",
            "--fork",
            "old",
            "--session-dir",
            "dir",
            "--session-id",
            "id",
            "--session",
            "file",
            "--no-session",
            "--no-supervisor",
        ],
        &["--refresh-models", "--no-supervisor", "--no-session"],
        &[
            "--mode",
            "json",
            "--mode",
            "invalid",
            "--thinking",
            "high",
            "--thinking",
            "invalid",
        ],
        &[
            "--append-system-prompt",
            "A",
            "--append-system-prompt",
            "B",
            "--extension",
            "a",
            "--extension",
            "b",
        ],
        &[
            "--unknown=one",
            "--other",
            "two",
            "--unknown=three",
            "--flag",
            "@file",
            "message",
        ],
    ];
    for (id, case) in cases.iter().enumerate() {
        let argv = case.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
        let expected = oracle.request(&json!({"id":id,"operation":"parse_args","args":argv}));
        assert_eq!(expected, response(id as u64, parse_args(&argv)), "{argv:?}");
    }
}

struct ParseCase {
    id: u64,
    name: &'static str,
    argv: Vec<String>,
}

fn parse_cases() -> Vec<ParseCase> {
    let raw: Vec<(u64, &'static str, &[&str])> = vec![
        (28_000, "empty", &[]),
        (28_001, "help and version aliases", &["--help", "-v", "-h", "--version"]),
        (
            28_002,
            "mode values",
            &["--mode", "text", "--mode", "json", "--mode", "rpc"],
        ),
        (28_003, "invalid and missing mode", &["--mode", "bogus", "--mode"]),
        (
            28_004,
            "continue and resume aliases",
            &["-c", "--resume", "-r", "--continue"],
        ),
        (
            28_005,
            "value flags",
            &[
                "--provider",
                "openai",
                "--model",
                "gpt-4o",
                "--api-key",
                "sk-test",
                "--system-prompt",
                "You are helpful",
            ],
        ),
        (
            28_006,
            "repeated append-system-prompt",
            &["--append-system-prompt", "A", "--append-system-prompt", "B"],
        ),
        (28_007, "missing value flags", &["--name", "--worktree", "-n", "-w"]),
        (
            28_008,
            "session flags",
            &[
                "--no-session",
                "--session",
                "/tmp/session.jsonl",
                "--session-id",
                "exact-id",
                "--fork",
                "abcd1234",
                "--session-dir",
                "/tmp/sessions",
            ],
        ),
        (28_009, "name aliases", &["-n", "named", "--name", ""]),
        (
            28_010,
            "model and tool lists",
            &[
                "--models",
                " a , b ,, c ",
                "--tools",
                " read , ,grep ",
                "-xt",
                "edit,,write",
            ],
        ),
        (28_011, "empty tool lists", &["--tools", "", "--exclude-tools", " , "]),
        (
            28_012,
            "permission prompt tool",
            &["--permission-prompt-tool", "mcp__srv__tool"],
        ),
        (28_013, "missing permission prompt tool", &["--permission-prompt-tool"]),
        (
            28_014,
            "approval presets",
            &[
                "--approval-preset",
                "ask-me",
                "--approval-preset",
                "llm-approved-deny",
                "--approval-preset",
                "llm-approved-ask",
                "--approval-preset",
                "never-ask-deny",
                "--approval-preset",
                "auto-approve",
            ],
        ),
        (28_015, "invalid approval preset", &["--approval-preset", "bogus"]),
        (
            28_016,
            "missing and flag-like approval preset",
            &["--approval-preset", "--approval-preset"],
        ),
        (
            28_017,
            "approval policies",
            &[
                "--approval-policy",
                "never",
                "--approval-policy",
                "on-request",
                "--approval-policy",
                "auto-approve",
            ],
        ),
        (28_018, "invalid approval policy", &["--approval-policy", "bogus"]),
        (28_019, "dangerous bypass", &["--dangerously-bypass-approvals"]),
        (
            28_020,
            "thinking levels",
            &["--thinking", "off", "--thinking", "xhigh", "--thinking", "ultra"],
        ),
        (28_021, "invalid thinking level", &["--thinking", "bogus"]),
        (28_022, "thinking without value", &["--thinking"]),
        (28_023, "print prompt consumption", &["-p", "hello", "--print", "---x"]),
        (
            28_024,
            "print with flag and file",
            &["-p", "--provider", "openai", "@prompt.md", "hi"],
        ),
        (28_025, "print without value", &["--print"]),
        (28_026, "export", &["--export", "session.jsonl"]),
        (28_027, "export missing value", &["--export"]),
        (
            28_028,
            "extensions",
            &["-e", "a.ts", "--extension", "b.ts", "--no-extensions"],
        ),
        (
            28_029,
            "resources",
            &[
                "--skill",
                "one",
                "--skill",
                "two",
                "--prompt-template",
                "p1",
                "--prompt-template",
                "p2",
                "--theme",
                "t1",
                "--theme",
                "t2",
            ],
        ),
        (
            28_030,
            "resource disablers",
            &[
                "-ns",
                "--no-prompt-templates",
                "-np",
                "--no-themes",
                "--no-context-files",
                "-nc",
            ],
        ),
        (
            28_031,
            "list models",
            &["--list-models", "sonnet", "--list-models", "--list-models", "@file"],
        ),
        (28_032, "metadata commands", &["tools", "extensions"]),
        (28_033, "login with provider", &["login", "openai-codex"]),
        (28_034, "login errors", &["login", "login", "-x", "login", "@file"]),
        (
            28_035,
            "tui modes",
            &["--tui-mode", "regular", "--tui-mode", "fullscreen"],
        ),
        (28_036, "invalid tui mode", &["--tui-mode", "compact"]),
        (
            28_037,
            "missing and flag-like tui mode",
            &["--tui-mode", "--tui-mode", "-x"],
        ),
        (
            28_038,
            "verbose and trust",
            &["--verbose", "--approve", "--no-approve", "-a", "-na"],
        ),
        (28_039, "offline", &["--offline"]),
        (28_040, "file arguments", &["@one", "@/two", "@", "message"]),
        (
            28_041,
            "unknown long flags",
            &[
                "--plan",
                "--plan=on",
                "--extra",
                "value",
                "--extra",
                "replaced",
                "--flag",
                "@file",
                "--empty=",
                "---triple",
            ],
        ),
        (28_042, "unknown short flags", &["-x", "-abc", "--", "-"]),
        (
            28_043,
            "unicode and control arguments",
            &["héllo", "\0", "--name", "é\0", "-p", "multi\nline"],
        ),
        (
            28_044,
            "combined invocation",
            &[
                "--provider",
                "anthropic",
                "--model",
                "claude-sonnet:high",
                "-p",
                "@prompt.md",
                "--thinking",
                "high",
                "--tools",
                "read,grep",
                "--no-extensions",
                "Do the task",
                "--unknown",
                "value",
            ],
        ),
    ];

    raw.into_iter()
        .map(|(id, name, argv)| ParseCase {
            id,
            name,
            argv: argv.iter().map(|value| (*value).to_owned()).collect(),
        })
        .collect()
}

fn help_cases() -> Vec<(u64, &'static str, Vec<ExtensionFlag>)> {
    vec![
        (28_100, "help without extension flags", Vec::new()),
        (
            28_101,
            "help with extension flags",
            vec![
                ExtensionFlag {
                    name: "plan".to_owned(),
                    description: Some("Enable plan mode".to_owned()),
                    kind: ExtensionFlagType::Boolean,
                    default: None,
                    extension_path: "/ext/plan.ts".into(),
                },
                ExtensionFlag {
                    name: "model-extra".to_owned(),
                    description: None,
                    kind: ExtensionFlagType::String,
                    default: None,
                    extension_path: "/ext/extra.ts".into(),
                },
            ],
        ),
    ]
}

fn oracle_flags(flags: &[ExtensionFlag]) -> Value {
    Value::Array(
        flags
            .iter()
            .map(|flag| {
                let mut value = json!({
                    "name": flag.name,
                    "type": match flag.kind {
                        ExtensionFlagType::Boolean => "boolean",
                        ExtensionFlagType::String => "string",
                    },
                    "extensionPath": flag.extension_path.to_utf8().expect("fixture paths are well-formed"),
                });
                if let Some(description) = &flag.description {
                    value["description"] = json!(description);
                }
                value
            })
            .collect(),
    )
}

fn response(id: u64, args: Args) -> Value {
    json!({"id": id, "ok": true, "output": args_json(&args)})
}

fn args_json(args: &Args) -> Value {
    let mut output = serde_json::Map::new();
    insert_string(&mut output, "provider", &args.provider);
    insert_string(&mut output, "model", &args.model);
    insert_string(&mut output, "apiKey", &args.api_key);
    insert_string(&mut output, "systemPrompt", &args.system_prompt);
    insert_strings(&mut output, "appendSystemPrompt", &args.append_system_prompt);
    if let Some(thinking) = args.thinking {
        output.insert("thinking".to_owned(), json!(thinking.as_str()));
    }
    insert_bool(&mut output, "continue", args.continue_session);
    insert_bool(&mut output, "resume", args.resume);
    insert_bool(&mut output, "help", args.help);
    insert_bool(&mut output, "version", args.version);
    if let Some(mode) = args.mode {
        output.insert("mode".to_owned(), json!(mode.as_str()));
    }
    insert_string(&mut output, "name", &args.name);
    insert_bool(&mut output, "noSession", args.no_session);
    insert_bool(&mut output, "noSupervisor", args.no_supervisor);
    insert_string(
        &mut output,
        "session",
        &args
            .session
            .as_ref()
            .map(|session| session.to_utf8().expect("argv fixtures are well-formed")),
    );
    insert_string(&mut output, "sessionId", &args.session_id);
    insert_string(&mut output, "fork", &args.fork);
    insert_string(&mut output, "sessionDir", &args.session_dir);
    insert_string(&mut output, "worktree", &args.worktree);
    insert_strings(&mut output, "models", &args.models);
    insert_strings(&mut output, "tools", &args.tools);
    insert_strings(&mut output, "excludeTools", &args.exclude_tools);
    insert_string(&mut output, "permissionPromptTool", &args.permission_prompt_tool);
    if let Some(preset) = args.approval_preset {
        output.insert("approvalPreset".to_owned(), json!(preset.as_str()));
    }
    insert_bool(&mut output, "noTools", args.no_tools);
    insert_bool(&mut output, "noBuiltinTools", args.no_builtin_tools);
    insert_strings(&mut output, "extensions", &args.extensions);
    insert_bool(&mut output, "noExtensions", args.no_extensions);
    insert_bool(&mut output, "print", args.print);
    insert_string(&mut output, "export", &args.export);
    insert_string(&mut output, "exportTerminal", &args.export_terminal);
    if let Some(columns) = args.export_columns {
        output.insert("exportColumns".to_owned(), json!(columns));
    }
    insert_bool(&mut output, "noSkills", args.no_skills);
    insert_strings(&mut output, "skills", &args.skills);
    insert_strings(&mut output, "promptTemplates", &args.prompt_templates);
    insert_bool(&mut output, "noPromptTemplates", args.no_prompt_templates);
    insert_strings(&mut output, "themes", &args.themes);
    insert_bool(&mut output, "noThemes", args.no_themes);
    insert_bool(&mut output, "noContextFiles", args.no_context_files);
    if let Some(list_models) = &args.list_models {
        let value = match list_models {
            ListModels::All => json!(true),
            ListModels::Search(search) => json!(search),
        };
        output.insert("listModels".to_owned(), value);
    }
    insert_bool(&mut output, "refreshModels", args.refresh_models);
    insert_bool(&mut output, "listTools", args.list_tools);
    insert_bool(&mut output, "listExtensions", args.list_extensions);
    insert_string(&mut output, "loginProvider", &args.login_provider);
    insert_bool(&mut output, "offline", args.offline);
    insert_bool(&mut output, "verbose", args.verbose);
    insert_bool(&mut output, "projectTrustOverride", args.project_trust_override);
    output.insert("messages".to_owned(), json!(args.messages));
    output.insert("fileArgs".to_owned(), json!(args.file_args));
    output.insert(
        "unknownFlags".to_owned(),
        Value::Array(
            args.unknown_flags
                .iter()
                .map(|(name, value)| {
                    let value = match value {
                        UnknownFlagValue::Boolean(flag) => json!(flag),
                        UnknownFlagValue::String(text) => json!(text),
                    };
                    json!([name, value])
                })
                .collect(),
        ),
    );
    output.insert(
        "diagnostics".to_owned(),
        Value::Array(args.diagnostics.iter().map(diagnostic_json).collect()),
    );
    Value::Object(output)
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Value {
    let kind = match diagnostic.kind {
        DiagnosticType::Warning => "warning",
        DiagnosticType::Error => "error",
    };
    json!({"type": kind, "message": diagnostic.message})
}

fn insert_string(output: &mut serde_json::Map<String, Value>, name: &str, value: &Option<String>) {
    if let Some(value) = value {
        output.insert(name.to_owned(), json!(value));
    }
}

fn insert_strings(output: &mut serde_json::Map<String, Value>, name: &str, value: &Option<Vec<String>>) {
    if let Some(value) = value {
        output.insert(name.to_owned(), json!(value));
    }
}

fn insert_bool(output: &mut serde_json::Map<String, Value>, name: &str, value: Option<bool>) {
    if let Some(value) = value {
        output.insert(name.to_owned(), json!(value));
    }
}

#[test]
fn mode_names_match_typescript_literals() {
    assert_eq!(Mode::Text.as_str(), "text");
    assert_eq!(Mode::Json.as_str(), "json");
    assert_eq!(Mode::Rpc.as_str(), "rpc");
}
