//! CLI argument parsing and help display (`cli/args.ts`).

use indexmap::IndexMap;
use pi_agent_core::harness::ThinkingLevel;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::config;
use crate::core::permissions::policy::is_approval_policy;
pub use crate::core::permissions::presets::ApprovalPresetName;
use crate::core::permissions::presets::{approval_preset_for_policy, is_approval_preset_name};
use crate::utils::chalk::Chalk;

const VALID_THINKING_LEVELS: [ThinkingLevel; 8] = [
    ThinkingLevel::Off,
    ThinkingLevel::Minimal,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
    ThinkingLevel::XHigh,
    ThinkingLevel::Max,
    ThinkingLevel::Ultra,
];

pub fn is_valid_thinking_level(level: &str) -> bool {
    VALID_THINKING_LEVELS
        .iter()
        .any(|valid_level| valid_level.as_str() == level)
}

pub fn parse_thinking_level(level: &str) -> Option<ThinkingLevel> {
    VALID_THINKING_LEVELS
        .iter()
        .copied()
        .find(|valid_level| valid_level.as_str() == level)
}

/// `Mode` output mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Json,
    Rpc,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Rpc => "rpc",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "rpc" => Some(Self::Rpc),
            _ => None,
        }
    }
}

/// `listModels?: string | true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListModels {
    All,
    Search(String),
}

/// Unknown long flag values captured for extension flag handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownFlagValue {
    Boolean(bool),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticType {
    Warning,
    Error,
}

impl DiagnosticType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticType,
    pub message: String,
}

fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        kind: DiagnosticType::Error,
        message: message.into(),
    }
}

fn warning(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        kind: DiagnosticType::Warning,
        message: message.into(),
    }
}

/// Parsed CLI arguments. Field order mirrors the TypeScript `Args` interface.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Args {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<Vec<String>>,
    pub thinking: Option<ThinkingLevel>,
    pub continue_session: Option<bool>,
    pub resume: Option<bool>,
    pub help: Option<bool>,
    pub version: Option<bool>,
    pub mode: Option<Mode>,
    pub name: Option<String>,
    pub no_session: Option<bool>,
    pub no_supervisor: Option<bool>,
    /// Native because the self-restart handoff replaces it with a persisted session path.
    pub session: Option<JsString>,
    pub session_id: Option<String>,
    pub fork: Option<String>,
    pub session_dir: Option<String>,
    pub worktree: Option<String>,
    pub models: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub exclude_tools: Option<Vec<String>>,
    pub permission_prompt_tool: Option<String>,
    pub approval_preset: Option<ApprovalPresetName>,
    pub no_tools: Option<bool>,
    pub no_builtin_tools: Option<bool>,
    pub extensions: Option<Vec<String>>,
    pub no_extensions: Option<bool>,
    pub print: Option<bool>,
    pub export: Option<String>,
    pub no_skills: Option<bool>,
    pub skills: Option<Vec<String>>,
    pub prompt_templates: Option<Vec<String>>,
    pub no_prompt_templates: Option<bool>,
    pub themes: Option<Vec<String>>,
    pub no_themes: Option<bool>,
    pub no_context_files: Option<bool>,
    pub list_models: Option<ListModels>,
    pub refresh_models: Option<bool>,
    pub list_tools: Option<bool>,
    pub list_extensions: Option<bool>,
    pub login_provider: Option<String>,
    pub offline: Option<bool>,
    pub verbose: Option<bool>,
    pub project_trust_override: Option<bool>,
    pub messages: Vec<String>,
    pub file_args: Vec<String>,
    /// Unknown flags in encounter order (`Map` insertion order in the source).
    pub unknown_flags: IndexMap<String, UnknownFlagValue>,
    pub diagnostics: Vec<Diagnostic>,
}

fn split_comma_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|part| pi_js_compat::string::js_trim(part).to_owned())
        .collect()
}

fn split_non_empty_list(value: &str) -> Vec<String> {
    split_comma_list(value)
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect()
}

fn set_unknown_flag(unknown_flags: &mut IndexMap<String, UnknownFlagValue>, name: String, value: UnknownFlagValue) {
    unknown_flags.insert(name, value);
}

/// `parseArgs`.
pub fn parse_args(args: &[String]) -> Args {
    let mut result = Args::default();
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        let arg = arg.as_str();
        let next = args.get(index + 1).map(String::as_str);
        index += 1;

        match arg {
            "--help" | "-h" => result.help = Some(true),
            "--version" | "-v" => result.version = Some(true),
            "--mode" if next.is_some() => {
                if let Some(mode) = Mode::parse(next.expect("guarded")) {
                    result.mode = Some(mode);
                }
                index += 1;
            }
            "--continue" | "-c" => result.continue_session = Some(true),
            "--resume" | "-r" => result.resume = Some(true),
            "--provider" if next.is_some() => {
                result.provider = next.map(str::to_owned);
                index += 1;
            }
            "--model" if next.is_some() => {
                result.model = next.map(str::to_owned);
                index += 1;
            }
            "--api-key" if next.is_some() => {
                result.api_key = next.map(str::to_owned);
                index += 1;
            }
            "--system-prompt" if next.is_some() => {
                result.system_prompt = next.map(str::to_owned);
                index += 1;
            }
            "--append-system-prompt" if next.is_some() => {
                result
                    .append_system_prompt
                    .get_or_insert_with(Vec::new)
                    .push(next.expect("guarded").to_owned());
                index += 1;
            }
            "--name" | "-n" => match next {
                Some(value) => {
                    result.name = Some(value.to_owned());
                    index += 1;
                }
                None => result.diagnostics.push(error("--name requires a value")),
            },
            "--no-session" => result.no_session = Some(true),
            "--no-supervisor" => result.no_supervisor = Some(true),
            "--session" if next.is_some() => {
                result.session = next.map(JsString::from);
                index += 1;
            }
            "--session-id" if next.is_some() => {
                result.session_id = next.map(str::to_owned);
                index += 1;
            }
            "--fork" if next.is_some() => {
                result.fork = next.map(str::to_owned);
                index += 1;
            }
            "--session-dir" if next.is_some() => {
                result.session_dir = next.map(str::to_owned);
                index += 1;
            }
            "--worktree" | "-w" => match next {
                Some(value) => {
                    result.worktree = Some(value.to_owned());
                    index += 1;
                }
                None => result.diagnostics.push(error("--worktree requires a value")),
            },
            "--models" if next.is_some() => {
                result.models = Some(split_comma_list(next.expect("guarded")));
                index += 1;
            }
            "--no-tools" | "-nt" => result.no_tools = Some(true),
            "--no-builtin-tools" | "-nbt" => result.no_builtin_tools = Some(true),
            "--tools" | "-t" if next.is_some() => {
                result.tools = Some(split_non_empty_list(next.expect("guarded")));
                index += 1;
            }
            "--exclude-tools" | "-xt" if next.is_some() => {
                result.exclude_tools = Some(split_non_empty_list(next.expect("guarded")));
                index += 1;
            }
            "--permission-prompt-tool" => match next {
                Some(value) => {
                    result.permission_prompt_tool = Some(value.to_owned());
                    index += 1;
                }
                None => result
                    .diagnostics
                    .push(error("--permission-prompt-tool requires a value")),
            },
            "--approval-preset" => {
                parse_approval_preset(args, &mut index, &mut result);
            }
            "--approval-policy" => {
                parse_approval_policy(args, &mut index, &mut result);
            }
            "--dangerously-bypass-approvals" => result.approval_preset = Some(ApprovalPresetName::AutoApprove),
            "--thinking" if next.is_some() => {
                let level = next.expect("guarded");
                match parse_thinking_level(level) {
                    Some(thinking) => result.thinking = Some(thinking),
                    None => result.diagnostics.push(warning(format!(
                        "Invalid thinking level \"{level}\". Valid values: {}",
                        VALID_THINKING_LEVELS
                            .iter()
                            .map(|valid_level| valid_level.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))),
                }
                index += 1;
            }
            "--print" | "-p" => {
                result.print = Some(true);
                if let Some(value) = next
                    && !value.starts_with('@')
                    && (!value.starts_with('-') || value.starts_with("---"))
                {
                    result.messages.push(value.to_owned());
                    index += 1;
                }
            }
            "--export" if next.is_some() => {
                result.export = next.map(str::to_owned);
                index += 1;
            }
            "--extension" | "-e" if next.is_some() => {
                result
                    .extensions
                    .get_or_insert_with(Vec::new)
                    .push(next.expect("guarded").to_owned());
                index += 1;
            }
            "--no-extensions" | "-ne" => result.no_extensions = Some(true),
            "--skill" if next.is_some() => {
                result
                    .skills
                    .get_or_insert_with(Vec::new)
                    .push(next.expect("guarded").to_owned());
                index += 1;
            }
            "--prompt-template" if next.is_some() => {
                result
                    .prompt_templates
                    .get_or_insert_with(Vec::new)
                    .push(next.expect("guarded").to_owned());
                index += 1;
            }
            "--theme" if next.is_some() => {
                result
                    .themes
                    .get_or_insert_with(Vec::new)
                    .push(next.expect("guarded").to_owned());
                index += 1;
            }
            "--no-skills" | "-ns" => result.no_skills = Some(true),
            "--no-prompt-templates" | "-np" => result.no_prompt_templates = Some(true),
            "--no-themes" => result.no_themes = Some(true),
            "--no-context-files" | "-nc" => result.no_context_files = Some(true),
            "--list-models" => {
                if let Some(value) = next.filter(|value| !value.starts_with('-') && !value.starts_with('@')) {
                    result.list_models = Some(ListModels::Search(value.to_owned()));
                    index += 1;
                } else {
                    result.list_models = Some(ListModels::All);
                }
            }
            "--refresh-models" => result.refresh_models = Some(true),
            "tools" => result.list_tools = Some(true),
            "extensions" => result.list_extensions = Some(true),
            "login" => {
                if let Some(provider) = next.filter(|value| !value.starts_with('-') && !value.starts_with('@')) {
                    result.login_provider = Some(provider.to_owned());
                    index += 1;
                } else {
                    result.diagnostics.push(error("login requires a provider"));
                }
            }
            "--verbose" => result.verbose = Some(true),
            "--approve" | "-a" => result.project_trust_override = Some(true),
            "--no-approve" | "-na" => result.project_trust_override = Some(false),
            "--offline" => result.offline = Some(true),
            _ if arg.starts_with('@') => result.file_args.push(arg[1..].to_owned()),
            _ if arg.starts_with("--") => parse_unknown_long_flag(args, &mut index, &mut result, arg),
            _ if arg.starts_with('-') => result.diagnostics.push(error(format!("Unknown option: {arg}"))),
            _ => result.messages.push(arg.to_owned()),
        }
    }

    result
        .diagnostics
        .extend(collect_standalone_worker_diagnostics(&result));
    result
}

fn collect_standalone_worker_diagnostics(parsed: &Args) -> Vec<Diagnostic> {
    if parsed.no_supervisor != Some(true) {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    if parsed.no_session != Some(true) {
        diagnostics.push(error("--no-supervisor requires --no-session"));
    }
    let incompatible: Vec<&str> = [
        (parsed.session.is_some(), "--session"),
        (parsed.session_id.is_some(), "--session-id"),
        (parsed.session_dir.is_some(), "--session-dir"),
        (parsed.fork.is_some(), "--fork"),
        (parsed.resume == Some(true), "--resume"),
        (parsed.continue_session == Some(true), "--continue"),
        (parsed.name.is_some(), "--name"),
        (parsed.export.is_some(), "--export"),
    ]
    .into_iter()
    .filter_map(|(present, flag)| present.then_some(flag))
    .collect();
    if !incompatible.is_empty() {
        diagnostics.push(error(format!(
            "--no-supervisor cannot be combined with {}",
            incompatible.join(", ")
        )));
    }
    diagnostics
}

fn parse_approval_preset(args: &[String], index: &mut usize, result: &mut Args) {
    let next = args.get(*index).map(String::as_str);
    if let Some(value) = next.filter(|value| !value.starts_with('-')) {
        if is_approval_preset_name(&value.into()) {
            result.approval_preset = ApprovalPresetName::parse(value);
        } else {
            result
                .diagnostics
                .push(error(format!("Invalid --approval-preset value: {value}")));
        }
        *index += 1;
    } else {
        result.diagnostics.push(error("--approval-preset requires a value"));
    }
}

fn parse_approval_policy(args: &[String], index: &mut usize, result: &mut Args) {
    let next = args.get(*index).map(String::as_str);
    if let Some(value) = next.filter(|value| !value.starts_with('-')) {
        if is_approval_policy(value) {
            let policy = crate::core::permissions::policy::ApprovalPolicy::parse(value).expect("validated");
            result.approval_preset = Some(approval_preset_for_policy(policy));
        } else {
            result
                .diagnostics
                .push(error(format!("Invalid --approval-policy value: {value}")));
        }
        *index += 1;
    } else {
        result.diagnostics.push(error("--approval-policy requires a value"));
    }
}

fn parse_unknown_long_flag(args: &[String], index: &mut usize, result: &mut Args, arg: &str) {
    if let Some(equals) = arg.find('=') {
        set_unknown_flag(
            &mut result.unknown_flags,
            arg[2..equals].to_owned(),
            UnknownFlagValue::String(arg[equals + 1..].to_owned()),
        );
        return;
    }

    let flag_name = arg[2..].to_owned();
    match args
        .get(*index)
        .filter(|value| !value.starts_with('-') && !value.starts_with('@'))
    {
        Some(value) => {
            set_unknown_flag(
                &mut result.unknown_flags,
                flag_name,
                UnknownFlagValue::String(value.clone()),
            );
            *index += 1;
        }
        None => set_unknown_flag(&mut result.unknown_flags, flag_name, UnknownFlagValue::Boolean(true)),
    }
}

/// `ExtensionFlag` as consumed by `printHelp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionFlag {
    pub name: String,
    pub description: Option<String>,
    pub kind: ExtensionFlagType,
    pub default: Option<ExtensionFlagDefault>,
    pub extension_path: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionFlagType {
    Boolean,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionFlagDefault {
    Boolean(bool),
    String(String),
}

pub fn pad_end_utf16(value: &str, width: usize) -> String {
    let length = pi_js_compat::string::utf16_len(value);
    let mut padded = value.to_owned();
    for _ in length..width {
        padded.push(' ');
    }
    padded
}

fn extension_flags_text(extension_flags: &[ExtensionFlag], chalk: &Chalk) -> JsString {
    if extension_flags.is_empty() {
        return JsString::default();
    }

    let rows = extension_flags.iter().map(|flag| {
        let value = match flag.kind {
            ExtensionFlagType::String => " <value>",
            ExtensionFlagType::Boolean => "",
        };
        let description = match &flag.description {
            Some(description) => JsString::from(description.as_str()),
            None => js_format!("Registered by {}", flag.extension_path),
        };
        JsString::from(pad_end_utf16(&format!("  --{}{value}", flag.name), 30)) + description
    });
    js_format!(
        "\n{}\n{}\n",
        chalk.bold("Extension CLI Flags:"),
        JsString::join(rows, "\n")
    )
}

/// `printHelp` output including the trailing newline from `console.log`. This is the stdout
/// boundary: each native piece is encoded separately, which equals encoding the whole text
/// because every piece is adjacent only to ASCII.
pub fn print_help(extension_flags: &[ExtensionFlag]) -> String {
    let chalk = Chalk::stdout();
    let mut output = render_help(extension_flags, &chalk);
    output.push('\n');
    output
}

fn render_help(extension_flags: &[ExtensionFlag], chalk: &Chalk) -> String {
    let metadata = config::package_config();
    let app_name = encode_node_utf8(&metadata.app_name);
    let config_dir_name = encode_node_utf8(&metadata.config_dir_name);
    let prefix = metadata.app_name.to_uppercase();
    let env_agent_dir = encode_node_utf8(&js_format!("{}{}", prefix, config::ENV_AGENT_DIR_SUFFIX).pad_end(32));
    let env_session_dir = encode_node_utf8(&js_format!("{}{}", prefix, config::ENV_SESSION_DIR_SUFFIX).pad_end(32));
    let extension_flags_text = encode_node_utf8(&extension_flags_text(extension_flags, chalk));
    let bold = |text: &str| encode_node_utf8(&chalk.bold(text));

    format!(
        "{app} - AI coding assistant with file, command, and structural code tools

{usage}
  {app} [options] [@files...] [messages...]

{commands}
  {app} install <source> [-l]     Install extension source and add to settings
  {app} remove <source> [-l]      Remove extension source from settings
  {app} uninstall <source> [-l]   Alias for remove
  {app} update [source|self|pi]   Update pi (use --all for pi and extensions)
  {app} list                      List installed extensions from settings
  {app} login <provider>          Log in to an OAuth provider (use --browser-cli to open auth URLs)
  {app} debug attach <session-id> Attach to an enabled live-process debug REPL
  {app} sessions archive           Archive sessions older than five days
  {app} sessions truncate-tool-output  Truncate oversized persisted tool results
  {app} tools                     List tools available to the current session
  {app} extensions                List extensions loaded for the current session
  {app} config                    Open TUI to enable/disable package resources
  {app} <command> --help          Show help for install/remove/uninstall/update/list

{options}
  --provider <name>              Provider name (default: google)
  --model <pattern>              Model pattern or ID (supports \"provider/id\" and optional \":<thinking>\")
  --api-key <key>                API key (defaults to env vars)
  --system-prompt <text>         System prompt (default: coding assistant prompt)
  --append-system-prompt <text>  Append text or file contents to the system prompt (can be used multiple times)
  --mode <mode>                  Output mode: text (default), json, or rpc
  --print, -p                    Non-interactive mode: process prompt and exit
  --continue, -c                 Continue previous session
  --resume, -r                   Select a session to resume
  --session <path|id>            Use specific session file or partial UUID
  --session-id <id>              Use exact project session ID, creating it if missing
  --fork <path|id>               Fork specific session file or partial UUID into a new session
  --session-dir <dir>            Directory for session storage and lookup
  --worktree, -w <name>          Create or reuse a sibling git worktree before startup
  --no-session                   Don't save session (ephemeral)
  --no-supervisor                Standalone worker; requires --no-session; no orchestration or Supervisor
  --name, -n <name>              Set session display name
  --models <patterns>            Comma-separated model patterns for Ctrl+P cycling
                                 Supports globs (anthropic/*, *sonnet*) and fuzzy matching
  --no-tools, -nt                Disable all tools by default (built-in and extension)
  --no-builtin-tools, -nbt       Disable built-in tools by default but keep extension/custom tools enabled
  --tools, -t <tools>            Comma-separated allowlist of tool names to enable
                                 Applies to built-in, extension, and custom tools
  --exclude-tools, -xt <tools>   Comma-separated denylist of tool names to disable
                                 Applies to built-in, extension, and custom tools
  --permission-prompt-tool <mcp__server__tool>
                                 Delegate tool approval decisions to an MCP tool
  --thinking <level>             Set thinking level: off, minimal, low, medium, high, xhigh, max, ultra
  --extension, -e <path>         Load an extension file (can be used multiple times)
  --no-extensions, -ne           Disable extension discovery (explicit -e paths still work)
  --skill <path>                 Load a skill file or directory (can be used multiple times)
  --no-skills, -ns               Disable skills discovery and loading
  --prompt-template <path>       Load a prompt template file or directory (can be used multiple times)
  --no-prompt-templates, -np     Disable prompt template discovery and loading
  --theme <path>                 Load a theme file or directory (can be used multiple times)
  --no-themes                    Disable theme discovery and loading
  --no-context-files, -nc        Disable AGENTS.md, AGENTS.local.md, CLAUDE.md, and CLAUDE.local.md discovery/loading
  --export <file>                Export session file to HTML and exit
  --export-terminal <file|id>    Read-only ANSI history to stdout (exit 4: session ID not found)
  --export-columns <columns>     Positive integer width, required with --export-terminal
  --list-models [search]         List available models (with optional fuzzy search)
  --refresh-models               Refresh the cached OpenRouter model catalog and exit
  --verbose                      Force verbose startup (overrides quietStartup setting)
  --approve, -a                  Trust project-local files for this run
  --no-approve, -na              Ignore project-local files for this run
  --offline                      Disable startup network operations (same as PI_OFFLINE=1)
  --help, -h                     Show this help
  --version, -v                  Show version number

Extensions can register additional flags (e.g., --plan from plan-mode extension).{extension_flags_text}

{examples}
  # Interactive mode
  {app}

  # Interactive mode with initial prompt
  {app} \"List all .ts files in src/\"

  # Include files in initial message
  {app} @prompt.md @image.png \"What color is the sky?\"

  # Non-interactive mode (process and exit)
  {app} -p \"List all .ts files in src/\"

  # Multiple messages (interactive)
  {app} \"Read package.json\" \"What dependencies do we have?\"

  # Continue previous session
  {app} --continue \"What did we discuss?\"

  # Start a named session
  {app} --name \"Refactor auth module\"

  # Use different model
  {app} --provider openai --model gpt-4o-mini \"Help me refactor this code\"

  # Use model with provider prefix (no --provider needed)
  {app} --model openai/gpt-4o \"Help me refactor this code\"

  # Use model with thinking level shorthand
  {app} --model sonnet:high \"Solve this complex problem\"

  # Limit model cycling to specific models
  {app} --models claude-sonnet,claude-haiku,gpt-4o

  # Limit to a specific provider with glob pattern
  {app} --models \"github-copilot/*\"

  # Cycle models with fixed thinking levels
  {app} --models sonnet:high,haiku:low

  # Start with a specific thinking level
  {app} --thinking high \"Solve this complex problem\"

  # Read-only mode (no file modifications possible)
  {app} --tools read,grep,find,ls -p \"Review the code in src/\"

  # Disable one tool while keeping the rest available
  {app} --exclude-tools ask_question

  # Export a session file to HTML
  {app} --export ~/{config_dir_name}/agent/sessions/--path--/session.jsonl
  {app} --export session.jsonl output.html

{environment}
  ANTHROPIC_API_KEY                - Anthropic Claude API key
  ANTHROPIC_OAUTH_TOKEN            - Anthropic OAuth token (alternative to API key)
  ANT_LING_API_KEY                 - Ant Ling API key
  OPENAI_API_KEY                   - OpenAI GPT API key
  AZURE_OPENAI_API_KEY             - Azure OpenAI API key
  AZURE_OPENAI_BASE_URL            - Azure OpenAI/Cognitive Services base URL (e.g. https://{{resource}}.openai.azure.com)
  AZURE_OPENAI_RESOURCE_NAME       - Azure OpenAI resource name (alternative to base URL)
  AZURE_OPENAI_API_VERSION         - Azure OpenAI API version (default: v1)
  AZURE_OPENAI_DEPLOYMENT_NAME_MAP - Azure OpenAI model=deployment map (comma-separated)
  DEEPSEEK_API_KEY                 - DeepSeek API key
  NVIDIA_API_KEY                   - NVIDIA NIM API key
  GEMINI_API_KEY                   - Google Gemini API key
  GROQ_API_KEY                     - Groq API key
  CEREBRAS_API_KEY                 - Cerebras API key
  XAI_API_KEY                      - xAI Grok API key
  FIREWORKS_API_KEY                - Fireworks API key
  TOGETHER_API_KEY                 - Together AI API key
  OPENROUTER_API_KEY               - OpenRouter API key
  AI_GATEWAY_API_KEY               - Vercel AI Gateway API key
  ZAI_API_KEY                      - ZAI Coding Plan API key (Global)
  ZAI_CODING_CN_API_KEY            - ZAI Coding Plan API key (China)
  MISTRAL_API_KEY                  - Mistral API key
  MINIMAX_API_KEY                  - MiniMax API key
  MOONSHOT_API_KEY                 - Moonshot AI API key
  OPENCODE_API_KEY                 - OpenCode Zen/OpenCode Go API key
  KIMI_API_KEY                     - Kimi For Coding API key
  CLOUDFLARE_API_KEY               - Cloudflare API token (Workers AI and AI Gateway)
  CLOUDFLARE_ACCOUNT_ID            - Cloudflare account id (required for both)
  CLOUDFLARE_GATEWAY_ID            - Cloudflare AI Gateway slug (required for AI Gateway)
  XIAOMI_API_KEY                   - Xiaomi MiMo API key (api.xiaomimimo.com billing)
  XIAOMI_TOKEN_PLAN_CN_API_KEY     - Xiaomi MiMo Token Plan API key (China region)
  XIAOMI_TOKEN_PLAN_AMS_API_KEY    - Xiaomi MiMo Token Plan API key (Amsterdam region)
  XIAOMI_TOKEN_PLAN_SGP_API_KEY    - Xiaomi MiMo Token Plan API key (Singapore region)
  AWS_PROFILE                      - AWS profile for Amazon Bedrock
  AWS_ACCESS_KEY_ID                - AWS access key for Amazon Bedrock
  AWS_SECRET_ACCESS_KEY            - AWS secret key for Amazon Bedrock
  AWS_BEARER_TOKEN_BEDROCK         - Bedrock API key (bearer token)
  AWS_REGION                       - AWS region for Amazon Bedrock (e.g., us-east-1)
  {env_agent_dir} - Config directory (default: ~/{config_dir_name}/agent)
  {env_session_dir} - Session storage directory (overridden by --session-dir)
  PI_PACKAGE_DIR                   - Override package directory (for Nix/Guix store paths)
  PI_OFFLINE                       - Disable startup network operations when set to 1/true/yes
  PI_TELEMETRY                     - Override install telemetry when set to 1/true/yes or 0/false/no
  PI_SHARE_VIEWER_URL              - Base URL for /share command (default: https://pi.dev/session/)

{tools}
  read   - Read file contents
  bash   - Execute bash commands
  edit   - Edit files with find/replace
  write  - Write files (creates/overwrites)
  grep   - Search file contents (read-only, off by default)
  find   - Find files by glob pattern (read-only, off by default)
  ls     - List directory contents (read-only, off by default)
",
        app = app_name,
        usage = bold("Usage:"),
        commands = bold("Commands:"),
        options = bold("Options:"),
        examples = bold("Examples:"),
        environment = bold("Environment Variables:"),
        tools = bold("Built-in Tool Names:"),
    )
}
