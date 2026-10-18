//! `pi login` command (`cli/login-command.ts`).

use std::io::IsTerminal;
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    AuthEvent, AuthInfoLink, AuthInteraction, AuthPrompt, AuthPromptKind, AuthSelectOption, AuthType,
};
use pi_ai::models::Provider;
use pi_ai::request::{BoxFuture, CallbackError};
use tokio_util::sync::CancellationToken;

use crate::core::model_runtime::ModelRuntime;
use crate::utils::chalk::Chalk;

/// Opens one OAuth URL through `browser-cli`.
pub type BrowserCliOpener = Arc<dyn Fn(String) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

/// Terminal IO used by the login flow.
pub trait LoginIo: Send + Sync {
    fn stdout(&self, message: &str);
    fn stderr(&self, message: &str);
    /// Write `prompt`, then return one input line (without its newline).
    fn read_line<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, String>>;
}

/// Process stdio: prompts read one line on a TTY, or consume buffered lines
/// collected from piped stdin like the source's readline fallback.
pub struct StdioLoginIo {
    is_tty: bool,
    lines: Mutex<Option<Vec<String>>>,
    next_line: Mutex<usize>,
}

impl StdioLoginIo {
    pub fn new() -> Self {
        Self {
            is_tty: std::io::stdin().is_terminal(),
            lines: Mutex::new(None),
            next_line: Mutex::new(0),
        }
    }

    fn next_buffered_line(&self) -> String {
        let mut lines = self.lines.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let lines = lines.get_or_insert_with(read_stdin_lines);
        let mut index = self.next_line.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let line = lines.get(*index).cloned().unwrap_or_default();
        *index += 1;
        line
    }
}

impl Default for StdioLoginIo {
    fn default() -> Self {
        Self::new()
    }
}

fn read_stdin_lines() -> Vec<String> {
    use std::io::Read;
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    split_lines(&text)
}

fn split_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect()
}

impl LoginIo for StdioLoginIo {
    fn stdout(&self, message: &str) {
        use std::io::Write;
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(message.as_bytes());
        let _ = stdout.flush();
    }

    fn stderr(&self, message: &str) {
        use std::io::Write;
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(message.as_bytes());
        let _ = stderr.flush();
    }

    fn read_line<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            self.stdout(prompt);
            if !self.is_tty {
                return Ok(self.next_buffered_line());
            }
            tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| error.to_string())?;
                Ok(line.trim_end_matches(['\r', '\n']).to_owned())
            })
            .await
            .map_err(|error| error.to_string())?
        })
    }
}

/// Options for [`run_login_command`].
pub struct RunLoginCommandOptions {
    pub model_runtime: ModelRuntime,
    pub provider_id: String,
    pub io: Arc<dyn LoginIo>,
    pub use_browser_cli: bool,
    pub open_browser_cli_url: Option<BrowserCliOpener>,
}

fn oauth_providers(model_runtime: &ModelRuntime) -> Vec<Arc<dyn Provider>> {
    model_runtime
        .get_providers()
        .into_iter()
        .filter(|provider| provider.auth().oauth.is_some())
        .collect()
}

fn format_provider_list(model_runtime: &ModelRuntime) -> String {
    oauth_providers(model_runtime)
        .iter()
        .map(|provider| format!("  {} {}", pad_end(provider.id(), 20), provider.name()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn pad_end(value: &str, width: usize) -> String {
    let mut padded = value.to_owned();
    for _ in pi_js_compat::string::utf16_len(value)..width {
        padded.push(' ');
    }
    padded
}

fn print_login_usage(io: &dyn LoginIo, model_runtime: &ModelRuntime) {
    io.stderr("Usage: pi login <provider> [--browser-cli]\n");
    io.stderr(&format!(
        "Available OAuth providers:\n{}\n",
        format_provider_list(model_runtime)
    ));
}

fn select_option<'a>(
    io: &'a dyn LoginIo,
    message: &'a str,
    options: &'a [AuthSelectOption],
) -> BoxFuture<'a, Result<String, String>> {
    Box::pin(async move {
        io.stdout(&format!("{message}\n"));
        for (index, option) in options.iter().enumerate() {
            io.stdout(&format!("  {}. {} ({})\n", index + 1, option.label, option.id));
        }

        let default_option_id = options.first().map(|option| option.id.as_str());
        let answer = pi_js_compat::string::js_trim(
            &io.read_line(&format!(
                "Enter number or id (default: {}): ",
                default_option_id.unwrap_or("none")
            ))
            .await?,
        )
        .to_owned();
        if answer.is_empty() {
            return Ok(default_option_id.unwrap_or_default().to_owned());
        }

        let selected_index = pi_js_compat::number::parse_int(&answer, 10) - 1.0;
        if selected_index.is_finite()
            && selected_index >= 0.0
            && selected_index.fract() == 0.0
            && let Some(option) = options.get(selected_index as usize)
        {
            return Ok(option.id.clone());
        }

        Ok(options
            .iter()
            .find(|option| option.id == answer)
            .map(|option| option.id.clone())
            .unwrap_or_default())
    })
}

fn prompt_message(prompt: &AuthPrompt) -> String {
    match &prompt.kind {
        AuthPromptKind::Text { message, placeholder }
        | AuthPromptKind::Secret { message, placeholder }
        | AuthPromptKind::ManualCode { message, placeholder } => format_prompt(message, placeholder.as_deref()),
        AuthPromptKind::Select { message, .. } => message.clone(),
    }
}

fn format_prompt(message: &str, placeholder: Option<&str>) -> String {
    match placeholder {
        Some(placeholder) => format!("{message} ({placeholder}): "),
        None => format!("{message}: "),
    }
}

fn prompt_for_auth<'a>(io: &'a dyn LoginIo, prompt: &'a AuthPrompt) -> BoxFuture<'a, Result<String, String>> {
    Box::pin(async move {
        match &prompt.kind {
            AuthPromptKind::Select { message, options } => select_option(io, message, options).await,
            _ => io.read_line(&prompt_message(prompt)).await,
        }
    })
}

fn print_info_event(event_message: &str, links: Option<&[AuthInfoLink]>, io: &dyn LoginIo) {
    io.stdout(&format!("{event_message}\n"));
    for link in links.unwrap_or_default() {
        match &link.label {
            Some(label) => io.stdout(&format!("{label}: {}\n", link.url)),
            None => io.stdout(&format!("{}\n", link.url)),
        }
    }
}

fn print_auth_instructions(event: &AuthEvent, io: &dyn LoginIo) -> String {
    let (url, instructions, device_code) = match event {
        AuthEvent::AuthUrl { url, instructions } => (url.clone(), instructions.as_deref(), None),
        AuthEvent::DeviceCode {
            user_code,
            verification_uri,
            expires_in_seconds,
            ..
        } => (verification_uri.clone(), None, Some((user_code, *expires_in_seconds))),
        _ => return String::new(),
    };

    io.stdout("\n");
    io.stdout("Open this URL in your browser:\n");
    io.stdout(&format!("{url}\n"));
    if let Some(instructions) = instructions {
        io.stdout(&format!("{instructions}\n"));
    }
    if let Some((user_code, expires_in_seconds)) = device_code {
        io.stdout(&format!("Enter code: {user_code}\n"));
        if let Some(expires_in_seconds) = expires_in_seconds {
            io.stdout(&format!(
                "Code expires in {} seconds.\n",
                pi_js_compat::number::js_number_to_string(expires_in_seconds)
            ));
        }
    }
    url
}

fn notify_auth_event(
    event: &AuthEvent,
    io: &Arc<dyn LoginIo>,
    browser_cli_opener: Option<&BrowserCliOpener>,
    browser_cli_open_tasks: &Mutex<Vec<tokio::task::JoinHandle<()>>>,
) {
    match event {
        AuthEvent::Progress { message } => {
            io.stdout(&format!("{message}\n"));
            return;
        }
        AuthEvent::Info { message, links } => {
            print_info_event(message, links.as_deref(), io.as_ref());
            return;
        }
        _ => {}
    }

    let url = print_auth_instructions(event, io.as_ref());
    if let Some(opener) = browser_cli_opener {
        let opener = Arc::clone(opener);
        let io = Arc::clone(io);
        let handle = tokio::spawn(async move {
            match opener(url).await {
                Ok(()) => io.stdout("Opened auth URL with browser-cli.\n"),
                Err(error) => io.stdout(&format!("browser-cli open failed: {error}\n")),
            }
        });
        browser_cli_open_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(handle);
    }
    io.stdout("\n");
}

struct CliAuthInteraction {
    io: Arc<dyn LoginIo>,
    signal: CancellationToken,
    browser_cli_opener: Option<BrowserCliOpener>,
    browser_cli_open_tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl AuthInteraction for CliAuthInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, CallbackError>> {
        Box::pin(async move {
            let io = Arc::clone(&self.io);
            prompt_for_auth(io.as_ref(), &prompt).await.map_err(CallbackError::new)
        })
    }

    fn notify(&self, event: AuthEvent) {
        notify_auth_event(
            &event,
            &self.io,
            self.browser_cli_opener.as_ref(),
            &self.browser_cli_open_tasks,
        );
    }
}

async fn run_login_with_prompt(
    model_runtime: &ModelRuntime,
    provider_id: &str,
    io: Arc<dyn LoginIo>,
    browser_cli_opener: Option<BrowserCliOpener>,
) -> Result<(), String> {
    let interaction = Arc::new(CliAuthInteraction {
        io,
        signal: CancellationToken::new(),
        browser_cli_opener,
        browser_cli_open_tasks: Mutex::new(Vec::new()),
    });
    model_runtime
        .login(provider_id, AuthType::Oauth, interaction.clone())
        .await
        .map_err(|error| error.to_string())?;

    let tasks = std::mem::take(
        &mut *interaction
            .browser_cli_open_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    for task in tasks {
        let _ = task.await;
    }
    Ok(())
}

/// Default browser opener: `browser-cli open <url>`.
pub fn open_url_with_browser_cli(url: String) -> BoxFuture<'static, Result<(), String>> {
    Box::pin(async move {
        let mut child = crate::utils::child_process::spawn_process("browser-cli", &["open", &url])
            .map_err(|error| error.to_string())?;
        let output = crate::utils::child_process::wait_for_child_process(&mut child)
            .await
            .map_err(|error| error.to_string())?;
        if output.status == Some(0) {
            return Ok(());
        }
        Err(format!(
            "browser-cli open failed with exit code {}",
            output
                .status
                .map_or_else(|| "unknown".to_owned(), |status| status.to_string())
        ))
    })
}

/// `runLoginCommand`.
pub async fn run_login_command(options: RunLoginCommandOptions) -> i32 {
    let provider = options.model_runtime.get_provider(&options.provider_id);
    let provider = provider.filter(|provider| provider.auth().oauth.is_some());
    let Some(provider) = provider else {
        options
            .io
            .stderr(&Chalk::stderr().red(&format!("Unknown OAuth provider: {}\n", options.provider_id)));
        options.io.stderr(&format!(
            "Available OAuth providers:\n{}\n",
            format_provider_list(&options.model_runtime)
        ));
        return 1;
    };

    options.io.stdout(&format!(
        "Logging in to {} ({})...\n",
        provider.name(),
        options.provider_id
    ));
    let browser_cli_opener = options.use_browser_cli.then(|| {
        options
            .open_browser_cli_url
            .clone()
            .unwrap_or_else(|| Arc::new(open_url_with_browser_cli))
    });

    match run_login_with_prompt(
        &options.model_runtime,
        &options.provider_id,
        Arc::clone(&options.io),
        browser_cli_opener,
    )
    .await
    {
        Ok(()) => {
            options
                .io
                .stdout(&Chalk::stdout().green(&format!("Credentials saved for {}.\n", options.provider_id)));
            0
        }
        Err(message) => {
            options
                .io
                .stderr(&Chalk::stderr().red(&format!("Failed to login to {}: {message}\n", provider.name())));
            1
        }
    }
}

/// Dependencies for [`handle_login_command`].
pub struct LoginCommandDependencies {
    pub model_runtime: Option<ModelRuntime>,
    pub io: Option<Arc<dyn LoginIo>>,
    pub use_browser_cli: Option<bool>,
    pub open_browser_cli_url: Option<BrowserCliOpener>,
}

/// Result of handling one CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginCommandOutcome {
    pub handled: bool,
    pub exit_code: Option<i32>,
}

/// `handleLoginCommand`.
pub async fn handle_login_command(
    args: &[String],
    dependencies: LoginCommandDependencies,
) -> Result<LoginCommandOutcome, String> {
    if args.first().map(String::as_str) != Some("login") {
        return Ok(LoginCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }

    let parsed = parse_login_arguments(args);
    let io = dependencies.io.unwrap_or_else(|| Arc::new(StdioLoginIo::new()));
    let model_runtime = match dependencies.model_runtime {
        Some(model_runtime) => model_runtime,
        None => ModelRuntime::create(Default::default())
            .await
            .map_err(|error| error.to_string())?,
    };

    if parsed.provider_id.is_none() || parsed.has_help_flag || parsed.has_unknown_flags {
        print_login_usage(io.as_ref(), &model_runtime);
        return Ok(LoginCommandOutcome {
            handled: true,
            exit_code: Some(if parsed.has_help_flag { 0 } else { 1 }),
        });
    }

    let exit_code = run_login_command(RunLoginCommandOptions {
        model_runtime,
        provider_id: parsed.provider_id.unwrap_or_default(),
        io,
        use_browser_cli: dependencies.use_browser_cli.unwrap_or(parsed.use_browser_cli),
        open_browser_cli_url: dependencies.open_browser_cli_url,
    })
    .await;
    Ok(LoginCommandOutcome {
        handled: true,
        exit_code: Some(exit_code),
    })
}

struct ParsedLoginArguments {
    provider_id: Option<String>,
    has_help_flag: bool,
    use_browser_cli: bool,
    has_unknown_flags: bool,
}

fn is_login_help_flag(argument: Option<&str>) -> bool {
    matches!(argument, Some("--help" | "-h"))
}

fn parse_login_arguments(args: &[String]) -> ParsedLoginArguments {
    let provider_id = args.get(1).cloned();
    let flag_args = args.get(2..).unwrap_or_default();
    ParsedLoginArguments {
        has_help_flag: is_login_help_flag(provider_id.as_deref())
            || flag_args.iter().any(|argument| is_login_help_flag(Some(argument))),
        use_browser_cli: flag_args.iter().any(|argument| argument == "--browser-cli"),
        has_unknown_flags: flag_args
            .iter()
            .any(|argument| !matches!(argument.as_str(), "--browser-cli" | "--help" | "-h")),
        provider_id,
    }
}
