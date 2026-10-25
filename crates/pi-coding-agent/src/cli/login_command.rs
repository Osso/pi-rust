//! Frozen `cli/login-command.ts`: OAuth login through AuthStorage.
use std::io::IsTerminal;
use std::sync::{Arc, Mutex};

use pi_ai::oauth::{OAuthAuthInfo, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectPrompt};
use pi_ai::request::{BoxFuture, CallbackError};
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

use crate::core::auth_storage::AuthStorage;
use crate::utils::chalk::Chalk;

pub type BrowserCliOpener = Arc<dyn Fn(JsString) -> BoxFuture<'static, Result<(), JsString>> + Send + Sync>;

pub trait LoginIo: Send + Sync {
    fn stdout(&self, message: &JsString);
    fn stderr(&self, message: &JsString);
    fn read_line<'a>(&'a self, prompt: &'a JsString) -> BoxFuture<'a, Result<JsString, JsString>>;
}

pub struct StdioLoginIo {
    is_tty: bool,
    lines: Mutex<Option<Result<Vec<JsString>, JsString>>>,
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
    fn next_buffered_line(&self) -> Result<JsString, JsString> {
        let mut lines = self.lines.lock().expect("login input lock poisoned");
        let lines = lines
            .get_or_insert_with(read_stdin_lines)
            .as_ref()
            .map_err(Clone::clone)?;
        let mut index = self.next_line.lock().expect("login input index lock poisoned");
        let line = lines.get(*index).cloned().unwrap_or_default();
        *index += 1;
        Ok(line)
    }
}
impl Default for StdioLoginIo {
    fn default() -> Self {
        Self::new()
    }
}
fn read_stdin_lines() -> Result<Vec<JsString>, JsString> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|error| JsString::from(error.to_string()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.replace("\r\n", "\n").split('\n').map(JsString::from).collect())
}
impl LoginIo for StdioLoginIo {
    fn stdout(&self, message: &JsString) {
        crate::main_runner::write_stdout(message);
    }
    fn stderr(&self, message: &JsString) {
        crate::main_runner::write_stderr(message);
    }
    fn read_line<'a>(&'a self, prompt: &'a JsString) -> BoxFuture<'a, Result<JsString, JsString>> {
        Box::pin(async move {
            self.stdout(prompt);
            if !self.is_tty {
                return self.next_buffered_line();
            }
            tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| JsString::from(error.to_string()))?;
                Ok(JsString::from(line.trim_end_matches(['\r', '\n'])))
            })
            .await
            .map_err(|error| JsString::from(error.to_string()))?
        })
    }
}

pub struct RunLoginCommandOptions {
    pub auth_storage: AuthStorage,
    pub provider_id: String,
    pub io: Arc<dyn LoginIo>,
    pub use_browser_cli: bool,
    pub open_browser_cli_url: Option<BrowserCliOpener>,
}
fn format_provider_list() -> JsString {
    JsString::join(
        pi_ai::oauth::get_oauth_providers().iter().map(|provider| {
            let id = JsString::from(provider.id());
            js_format!("  {} {}", id.pad_end(20), provider.name())
        }),
        "\n",
    )
}
fn print_login_usage(io: &dyn LoginIo) {
    io.stderr(&"Usage: pi login <provider> [--browser-cli]\n".into());
    io.stderr(&js_format!("Available OAuth providers:\n{}\n", format_provider_list()));
}

struct CliOAuthCallbacks {
    io: Arc<dyn LoginIo>,
    browser_cli_opener: Option<BrowserCliOpener>,
    browser_cli_open_tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}
impl OAuthLoginCallbacks for CliOAuthCallbacks {
    fn on_auth(&self, info: OAuthAuthInfo) -> Result<(), CallbackError> {
        self.io
            .stdout(&js_format!("\nOpen this URL in your browser:\n{}\n", info.url));
        if let Some(instructions) = info.instructions
            && !instructions.is_empty()
        {
            self.io.stdout(&js_format!("{}\n", instructions));
        }
        if let Some(opener) = &self.browser_cli_opener {
            let opener = Arc::clone(opener);
            let io = Arc::clone(&self.io);
            let task = tokio::spawn(async move {
                match opener(info.url).await {
                    Ok(()) => io.stdout(&"Opened auth URL with browser-cli.\n".into()),
                    Err(error) => io.stdout(&js_format!("browser-cli open failed: {}\n", error)),
                }
            });
            self.browser_cli_open_tasks
                .lock()
                .expect("browser task lock poisoned")
                .push(task);
        }
        self.io.stdout(&"\n".into());
        Ok(())
    }
    fn on_device_code(&self, info: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
        self.io.stdout(&js_format!(
            "\nOpen this URL in your browser:\n{}\nEnter code: {}\n",
            info.verification_uri,
            info.user_code
        ));
        if let Some(expires) = info.expires_in_seconds {
            self.io.stdout(&js_format!(
                "Code expires in {} seconds.\n",
                pi_js_compat::number::js_number_to_string(expires)
            ));
        }
        self.io.stdout(&"\n".into());
        Ok(())
    }
    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            let suffix = prompt
                .placeholder
                .filter(|placeholder| !placeholder.is_empty())
                .map(|placeholder| js_format!(" ({})", placeholder))
                .unwrap_or_default();
            self.io
                .read_line(&js_format!("{}{}: ", prompt.message, suffix))
                .await
                .map_err(CallbackError::new)
        })
    }
    fn on_manual_code_input(&self) -> Option<BoxFuture<'_, Result<JsString, CallbackError>>> {
        Some(Box::pin(async move {
            self.io
                .read_line(&"Paste authorization code or redirect URL: ".into())
                .await
                .map_err(CallbackError::new)
        }))
    }
    fn on_progress(&self, message: &JsString) -> Result<(), CallbackError> {
        self.io.stdout(&js_format!("{}\n", message));
        Ok(())
    }
    fn on_select(&self, prompt: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        Box::pin(async move {
            self.io.stdout(&js_format!("{}\n", prompt.message));
            for (index, option) in prompt.options.iter().enumerate() {
                self.io
                    .stdout(&js_format!("  {}. {} ({})\n", index + 1, option.label, option.id));
            }
            let default = prompt.options.first().map(|option| option.id.clone());
            let answer = self
                .io
                .read_line(&js_format!(
                    "Enter number or id (default: {}): ",
                    default.as_ref().unwrap_or(&"none".into())
                ))
                .await
                .map_err(CallbackError::new)?
                .trim();
            if answer.is_empty() {
                return Ok(default);
            }
            let index = pi_js_compat::number::parse_int(answer.units(), 10) - 1.0;
            let is_option_index = index.is_finite() && index >= 0.0 && index.fract() == 0.0;
            if is_option_index && let Some(option) = prompt.options.get(index as usize) {
                return Ok(Some(option.id.clone()));
            }
            Ok(prompt
                .options
                .iter()
                .find(|option| option.id == answer)
                .map(|option| option.id.clone()))
        })
    }
}
async fn run_login_with_prompt(
    options: &RunLoginCommandOptions,
    opener: Option<BrowserCliOpener>,
) -> Result<(), JsString> {
    let callbacks = Arc::new(CliOAuthCallbacks {
        io: Arc::clone(&options.io),
        browser_cli_opener: opener,
        browser_cli_open_tasks: Mutex::new(Vec::new()),
    });
    options
        .auth_storage
        .login(&options.provider_id, callbacks.clone())
        .await
        .map_err(|error| error.message().clone())?;
    let tasks = std::mem::take(
        &mut *callbacks
            .browser_cli_open_tasks
            .lock()
            .expect("browser task lock poisoned"),
    );
    for task in tasks {
        task.await.map_err(|error| JsString::from(error.to_string()))?;
    }
    Ok(())
}

pub fn open_url_with_browser_cli(url: JsString) -> BoxFuture<'static, Result<(), JsString>> {
    Box::pin(async move {
        let url = encode_node_utf8(&url);
        let mut child = crate::utils::child_process::spawn_process(
            "browser-cli",
            &["open", &url],
            crate::utils::child_process::ChildStdioOptions::CAPTURE,
        )
        .map_err(|error| JsString::from(error.to_string()))?;
        let output = crate::utils::child_process::wait_for_child_process(&mut child)
            .await
            .map_err(|error| JsString::from(error.to_string()))?;
        if output.status == Some(0) {
            return Ok(());
        }
        Err(js_format!(
            "browser-cli open failed with exit code {}",
            output
                .status
                .map_or_else(|| "unknown".to_owned(), |status| status.to_string())
        ))
    })
}
pub async fn run_login_command(options: RunLoginCommandOptions) -> i32 {
    let Some(provider) = pi_ai::oauth::get_oauth_provider(&options.provider_id) else {
        options
            .io
            .stderr(&(Chalk::stdout().red(js_format!("Unknown OAuth provider: {}", options.provider_id)) + "\n"));
        options
            .io
            .stderr(&js_format!("Available OAuth providers:\n{}\n", format_provider_list()));
        return 1;
    };
    options.io.stdout(&js_format!(
        "Logging in to {} ({})...\n",
        provider.name(),
        options.provider_id
    ));
    let opener = options.use_browser_cli.then(|| {
        options
            .open_browser_cli_url
            .clone()
            .unwrap_or_else(|| Arc::new(open_url_with_browser_cli))
    });
    match run_login_with_prompt(&options, opener).await {
        Ok(()) => {
            options
                .io
                .stdout(&(Chalk::stdout().green(js_format!("Credentials saved for {}.", options.provider_id)) + "\n"));
            0
        }
        Err(message) => {
            options.io.stderr(
                &(Chalk::stdout().red(js_format!("Failed to login to {}: {}", provider.name(), message)) + "\n"),
            );
            1
        }
    }
}

pub struct LoginCommandDependencies {
    pub auth_storage: Option<AuthStorage>,
    pub io: Option<Arc<dyn LoginIo>>,
    pub use_browser_cli: Option<bool>,
    pub open_browser_cli_url: Option<BrowserCliOpener>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginCommandOutcome {
    pub handled: bool,
    pub exit_code: Option<i32>,
}
pub async fn handle_login_command(
    args: &[String],
    dependencies: LoginCommandDependencies,
) -> Result<LoginCommandOutcome, pi_ai::request::CallbackError> {
    if args.first().map(String::as_str) != Some("login") {
        return Ok(LoginCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }
    let parsed = parse_login_arguments(args);
    let io = dependencies.io.unwrap_or_else(|| Arc::new(StdioLoginIo::new()));
    if parsed.provider_id.is_none() || parsed.has_help_flag || parsed.has_unknown_flags {
        print_login_usage(io.as_ref());
        return Ok(LoginCommandOutcome {
            handled: true,
            exit_code: Some(if parsed.has_help_flag { 0 } else { 1 }),
        });
    }
    let auth_storage = dependencies
        .auth_storage
        .map(Ok)
        .unwrap_or_else(|| AuthStorage::create(None))?;
    let exit_code = run_login_command(RunLoginCommandOptions {
        auth_storage,
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
    let provider_id = args.get(1).filter(|provider| !provider.is_empty()).cloned();
    let flags = args.get(2..).unwrap_or_default();
    ParsedLoginArguments {
        has_help_flag: is_login_help_flag(provider_id.as_deref())
            || flags.iter().any(|argument| is_login_help_flag(Some(argument))),
        use_browser_cli: flags.iter().any(|argument| argument == "--browser-cli"),
        has_unknown_flags: flags
            .iter()
            .any(|argument| !matches!(argument.as_str(), "--browser-cli" | "--help" | "-h")),
        provider_id,
    }
}
