//! Terminal login through the frozen OAuth registry and AuthStorage boundaries.
#[path = "support/login_command_oracle.rs"]
mod login_command_oracle;

#[test]
fn login_command_matches_actual_frozen_callbacks_flags_persistence_and_stdio() {
    login_command_oracle::compare(false);
}

#[test]
fn login_buffered_stdio_matches_frozen_crlf_and_final_bare_carriage_return() {
    login_command_oracle::compare(true);
}

#[test]
fn login_errors_use_stdout_color_authority_when_stdout_is_redirected() {
    login_command_oracle::compare_color_authority();
}

#[tokio::test]
#[ignore = "child process probe"]
async fn login_command_oracle_child() {
    login_command_oracle::child().await;
}

use pi_ai::auth::AuthError;
use pi_ai::oauth::{
    OAuthAuthInfo, OAuthCredentials, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthProviderInterface,
    OAuthSelectOption, OAuthSelectPrompt, register_oauth_provider,
};
use pi_ai::request::{BoxFuture, CallbackError};
use pi_coding_agent::cli::login_command::{
    LoginCommandDependencies, LoginIo, RunLoginCommandOptions, StdioLoginIo, run_login_command,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct TestIo {
    lines: Mutex<std::collections::VecDeque<JsString>>,
    stdout: Mutex<JsString>,
    stderr: Mutex<JsString>,
}
impl TestIo {
    fn new(lines: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            lines: Mutex::new(lines.iter().map(|line| (*line).into()).collect()),
            ..Default::default()
        })
    }
    fn output(&self) -> JsString {
        self.stdout.lock().unwrap().clone()
    }
    fn error(&self) -> JsString {
        self.stderr.lock().unwrap().clone()
    }
}
impl LoginIo for TestIo {
    fn stdout(&self, text: &JsString) {
        self.stdout.lock().unwrap().push_str(text);
    }
    fn stderr(&self, text: &JsString) {
        self.stderr.lock().unwrap().push_str(text);
    }
    fn read_line<'a>(&'a self, prompt: &'a JsString) -> BoxFuture<'a, Result<JsString, JsString>> {
        self.stdout(prompt);
        let answer = self.lines.lock().unwrap().pop_front().unwrap_or_default();
        Box::pin(async move { Ok(answer) })
    }
}
type Login =
    Box<dyn Fn(Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> + Send + Sync>;
struct TestOAuth {
    id: String,
    name: String,
    login: Mutex<Option<Login>>,
}
impl OAuthProviderInterface for TestOAuth {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn login(
        &self,
        callbacks: Arc<dyn OAuthLoginCallbacks>,
    ) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        self.login.lock().unwrap().take().expect("login runs once")(callbacks)
    }
    fn refresh_token(&self, credentials: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async move { Ok(credentials) })
    }
    fn get_api_key(&self, credentials: &OAuthCredentials) -> Result<JsString, CallbackError> {
        Ok(credentials.access.clone())
    }
}
fn register(id: &str, name: &str, login: Login) {
    register_oauth_provider(Arc::new(TestOAuth {
        id: id.into(),
        name: name.into(),
        login: Mutex::new(Some(login)),
    }));
}
fn storage() -> AuthStorage {
    AuthStorage::in_memory([] as [(&str, serde_json::Value); 0])
}
fn credential(access: JsString) -> OAuthCredentials {
    OAuthCredentials::new("refresh-token", access, 9999999999999.0)
}
fn options(id: &str, storage: AuthStorage, io: Arc<TestIo>) -> RunLoginCommandOptions {
    RunLoginCommandOptions {
        auth_storage: storage,
        provider_id: id.into(),
        io,
        use_browser_cli: false,
        open_browser_cli_url: None,
    }
}

#[tokio::test]
async fn runs_oauth_login_interactions_from_terminal_io_and_persists_credentials() {
    let id = "test-cli-login";
    register(
        id,
        "Test CLI Login",
        Box::new(|callbacks| {
            Box::pin(async move {
                callbacks.on_auth(OAuthAuthInfo {
                    url: "https://example.invalid/login".into(),
                    instructions: Some("Open browser".into()),
                })?;
                callbacks.on_device_code(OAuthDeviceCodeInfo {
                    verification_uri: "https://example.invalid/device".into(),
                    user_code: "ABCD-EFGH".into(),
                    interval_seconds: None,
                    expires_in_seconds: None,
                })?;
                callbacks.on_progress(&"Waiting for login".into())?;
                let typed = callbacks
                    .on_prompt(OAuthPrompt {
                        message: "Enter token".into(),
                        placeholder: Some("token".into()),
                        allow_empty: None,
                    })
                    .await
                    .map_err(AuthError::from)?;
                let manual = callbacks
                    .on_manual_code_input()
                    .unwrap()
                    .await
                    .map_err(AuthError::from)?;
                let selected = callbacks
                    .on_select(OAuthSelectPrompt {
                        message: "Select account:".into(),
                        options: vec![
                            OAuthSelectOption {
                                id: "first".into(),
                                label: "First Account".into(),
                            },
                            OAuthSelectOption {
                                id: "second".into(),
                                label: "Second Account".into(),
                            },
                        ],
                    })
                    .await
                    .map_err(AuthError::from)?
                    .unwrap();
                Ok(credential(pi_js_compat::js_format!(
                    "{}:{}:{}", typed, manual, selected
                )))
            })
        }),
    );
    let storage = storage();
    let io = TestIo::new(&["typed-token", "manual-code", "2"]);
    assert_eq!(run_login_command(options(id, storage.clone(), io.clone())).await, 0);
    for text in [
        "https://example.invalid/login",
        "ABCD-EFGH",
        "Waiting for login",
        "Credentials saved for test-cli-login",
    ] {
        assert!(io.output().contains(text), "{:?}", io.output());
    }
    let stored = storage.get(id).unwrap();
    assert_eq!(
        stored.get("access").unwrap().as_string().unwrap(),
        "typed-token:manual-code:second"
    );
    assert_eq!(stored.get("refresh").unwrap().as_string().unwrap(), "refresh-token");
}
#[tokio::test]
async fn browser_open_success_and_failure_do_not_change_login_success() {
    for failed in [false, true] {
        let id = if failed {
            "test-cli-browser-failure"
        } else {
            "test-cli-browser-success"
        };
        register(
            id,
            "Browser Fixture",
            Box::new(|callbacks| {
                Box::pin(async move {
                    callbacks.on_auth(OAuthAuthInfo {
                        url: "https://example.invalid/browser-login".into(),
                        instructions: None,
                    })?;
                    Ok(credential("browser-access".into()))
                })
            }),
        );
        let io = TestIo::new(&[]);
        let opened = Arc::new(Mutex::new(Vec::new()));
        let captured = opened.clone();
        let mut options = options(id, storage(), io.clone());
        options.use_browser_cli = true;
        options.open_browser_cli_url = Some(Arc::new(move |url| {
            captured.lock().unwrap().push(url);
            Box::pin(async move { if failed { Err("exit code 2".into()) } else { Ok(()) } })
        }));
        assert_eq!(run_login_command(options).await, 0);
        assert_eq!(
            *opened.lock().unwrap(),
            vec![JsString::from("https://example.invalid/browser-login")]
        );
        let expected = if failed {
            "browser-cli open failed: exit code 2"
        } else {
            "Opened auth URL with browser-cli."
        };
        assert!(io.output().contains(expected));
    }
}
#[tokio::test]
async fn returns_nonzero_and_prints_provider_list_for_unknown_providers() {
    let storage = storage();
    let io = TestIo::new(&[]);
    assert_eq!(
        run_login_command(options("missing-provider", storage.clone(), io.clone())).await,
        1
    );
    assert!(io.error().contains("Unknown OAuth provider: missing-provider"));
    assert!(io.error().contains("Available OAuth providers:"));
    assert!(storage.get("missing-provider").is_none());
}
#[tokio::test]
async fn reports_login_failures_with_provider_name_and_native_units() {
    let raw = JsString::from_utf16(vec![0xd800, 0x21, 0xdfff]);
    let error = raw.clone();
    register(
        "test-cli-raw-error",
        "Failing Provider",
        Box::new(move |_| {
            let error = error.clone();
            Box::pin(async move { Err(AuthError::failed(error)) })
        }),
    );
    let io = TestIo::new(&[]);
    assert_eq!(
        run_login_command(options("test-cli-raw-error", storage(), io.clone())).await,
        1
    );
    assert!(io.error().contains("Failed to login to Failing Provider:"));
    assert!(io.error().contains(&raw));
    assert!(!io.error().contains("�"));
}
#[tokio::test]
async fn prints_usage_for_help_and_unknown_flags() {
    for argv in [
        vec!["login", "--help"],
        vec!["login", "openai-codex", "--bogus"],
        vec!["login"],
    ] {
        let io = TestIo::new(&[]);
        let outcome = pi_coding_agent::cli::login_command::handle_login_command(
            &argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>(),
            LoginCommandDependencies {
                auth_storage: Some(storage()),
                io: Some(io.clone()),
                use_browser_cli: None,
                open_browser_cli_url: None,
            },
        )
        .await
        .unwrap();
        assert!(outcome.handled);
        assert_eq!(
            outcome.exit_code,
            Some(if argv.get(1).copied() == Some("--help") { 0 } else { 1 })
        );
        assert!(io.error().starts_with("Usage: pi login <provider> [--browser-cli]"));
    }
}
#[test]
fn default_stdio_io_is_constructible() {
    let _io = StdioLoginIo::default();
}
