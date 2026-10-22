//! Ports `packages/coding-agent/test/login-command.test.ts`.

use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    AuthError, AuthEvent, AuthInfoLink, AuthInteraction, AuthPrompt, AuthPromptKind, AuthSelectOption, OAuthAuth,
    OAuthCredential, ProviderAuth,
};
use pi_ai::models::Provider;
use pi_ai::request::{BoxFuture, StreamOptions};
use pi_ai::types::{Context, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_ai::{CredentialStore, SimpleStreamOptions};
use pi_coding_agent::cli::login_command::{
    LoginCommandDependencies, LoginIo, RunLoginCommandOptions, StdioLoginIo, run_login_command,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_js_compat::js_json::JsString;
use serde_json::{Map, Value};

struct TestIo {
    lines: Mutex<Vec<String>>,
    next_line: Mutex<usize>,
    stdout: Mutex<JsString>,
    stderr: Mutex<JsString>,
}

impl TestIo {
    fn new(lines: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            lines: Mutex::new(lines.iter().map(|line| (*line).to_owned()).collect()),
            next_line: Mutex::new(0),
            stdout: Mutex::new(JsString::default()),
            stderr: Mutex::new(JsString::default()),
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
    fn stdout(&self, message: &JsString) {
        self.stdout.lock().unwrap().push_str(message);
    }

    fn stderr(&self, message: &JsString) {
        self.stderr.lock().unwrap().push_str(message);
    }

    fn read_line<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, JsString>> {
        self.stdout.lock().unwrap().push_str(prompt);
        let mut index = self.next_line.lock().unwrap();
        let line = self.lines.lock().unwrap().get(*index).cloned().unwrap_or_default();
        *index += 1;
        Box::pin(async move { Ok(line) })
    }
}

type LoginFn =
    Box<dyn Fn(Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync>;

struct TestOAuth {
    name: String,
    login: Mutex<Option<LoginFn>>,
}

impl OAuthAuth for TestOAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let login = self.login.lock().unwrap().take().expect("login runs once");
        login(interaction)
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        _signal: tokio_util::sync::CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async move { Ok(credential) })
    }

    fn to_auth(
        &self,
        credential: OAuthCredential,
    ) -> BoxFuture<'static, Result<pi_ai::auth::types::ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(pi_ai::auth::types::ModelAuth {
                api_key: Some(credential.access_utf8()?),
                ..Default::default()
            })
        })
    }
}

struct TestProvider {
    id: String,
    name: String,
    auth: ProviderAuth,
}

impl Provider for TestProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        Vec::new()
    }

    fn stream(&self, _model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        unreachable!("login test provider never streams")
    }

    fn stream_simple(
        &self,
        _model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        unreachable!("login test provider never streams")
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn auth_storage() -> Arc<AuthStorage> {
    let data = Map::<String, Value>::new();
    Arc::new(AuthStorage::in_memory(data))
}

async fn create_runtime(storage: Arc<AuthStorage>) -> ModelRuntime {
    ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage),
        models_path: Some(None),
        allow_model_network: false,
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .unwrap()
}

fn oauth_provider(id: &str, name: &str, login: LoginFn) -> Arc<dyn Provider> {
    Arc::new(TestProvider {
        id: id.to_owned(),
        name: name.to_owned(),
        auth: ProviderAuth::oauth(Arc::new(TestOAuth {
            name: name.to_owned(),
            login: Mutex::new(Some(login)),
        })),
    })
}

fn prompt(kind: AuthPromptKind) -> AuthPrompt {
    AuthPrompt { signal: None, kind }
}

#[test]
fn runs_oauth_login_interactions_from_terminal_io_and_persists_credentials() {
    runtime().block_on(async {
        let provider_id = "test-cli-login";
        let storage = auth_storage();
        let model_runtime = create_runtime(storage.clone()).await;
        model_runtime.register_native_provider(oauth_provider(
            provider_id,
            "Test CLI Login",
            Box::new(|interaction| {
                Box::pin(async move {
                    interaction.notify(AuthEvent::AuthUrl {
                        url: "https://example.invalid/login".to_owned(),
                        instructions: Some("Open browser".to_owned()),
                    });
                    interaction.notify(AuthEvent::DeviceCode {
                        verification_uri: "https://example.invalid/device".to_owned(),
                        user_code: "ABCD-EFGH".to_owned(),
                        interval_seconds: None,
                        expires_in_seconds: None,
                    });
                    interaction.notify(AuthEvent::Progress {
                        message: "Waiting for login".to_owned(),
                    });
                    let typed = interaction
                        .prompt(prompt(AuthPromptKind::Text {
                            message: "Enter token".to_owned(),
                            placeholder: Some("token".to_owned()),
                        }))
                        .await
                        .map_err(AuthError::Failed)?;
                    let manual = interaction
                        .prompt(prompt(AuthPromptKind::ManualCode {
                            message: "Paste authorization code or redirect URL".to_owned(),
                            placeholder: None,
                        }))
                        .await
                        .map_err(AuthError::Failed)?;
                    let selected = interaction
                        .prompt(prompt(AuthPromptKind::Select {
                            message: "Select account:".to_owned(),
                            options: vec![
                                AuthSelectOption {
                                    id: "first".to_owned(),
                                    label: "First Account".to_owned(),
                                    description: None,
                                },
                                AuthSelectOption {
                                    id: "second".to_owned(),
                                    label: "Second Account".to_owned(),
                                    description: None,
                                },
                            ],
                        }))
                        .await
                        .map_err(AuthError::Failed)?;
                    Ok(OAuthCredential::new(
                        "refresh-token",
                        format!("{typed}:{manual}:{selected}"),
                        pi_ai::request::now_millis() as f64 + 60_000.0,
                    ))
                })
            }),
        ));

        let io = TestIo::new(&["typed-token", "manual-code", "2"]);
        let exit_code = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: provider_id.to_owned(),
            io: io.clone(),
            use_browser_cli: false,
            open_browser_cli_url: None,
        })
        .await;

        assert_eq!(exit_code, 0);
        let output = io.output();
        assert!(output.contains("https://example.invalid/login"), "{output}");
        assert!(output.contains("ABCD-EFGH"), "{output}");
        assert!(output.contains("Waiting for login"), "{output}");
        assert!(
            output.contains(&format!("Credentials saved for {provider_id}")),
            "{output}"
        );
        let stored = storage.read(provider_id, Default::default()).await.unwrap().unwrap();
        let oauth = stored.as_oauth().expect("stored credential is OAuth");
        assert_eq!(oauth.access.to_utf8().unwrap(), "typed-token:manual-code:second");
        assert_eq!(oauth.refresh.to_utf8().unwrap(), "refresh-token");
    });
}

#[test]
fn opens_browser_oauth_urls_when_requested() {
    runtime().block_on(async {
        let provider_id = "test-cli-login-browser";
        let storage = auth_storage();
        let model_runtime = create_runtime(storage).await;
        model_runtime.register_native_provider(oauth_provider(
            provider_id,
            "Test Browser CLI Login",
            Box::new(|interaction| {
                Box::pin(async move {
                    interaction.notify(AuthEvent::AuthUrl {
                        url: "https://example.invalid/browser-login".to_owned(),
                        instructions: None,
                    });
                    Ok(OAuthCredential::new(
                        "browser-refresh",
                        "browser-access",
                        pi_ai::request::now_millis() as f64 + 60_000.0,
                    ))
                })
            }),
        ));

        let io = TestIo::new(&[]);
        let opened: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let opened_by_opener = Arc::clone(&opened);
        let exit_code = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: provider_id.to_owned(),
            io: io.clone(),
            use_browser_cli: true,
            open_browser_cli_url: Some(Arc::new(move |url: String| {
                opened_by_opener.lock().unwrap().push(url);
                Box::pin(async { Ok(()) })
            })),
        })
        .await;

        assert_eq!(exit_code, 0);
        assert_eq!(
            *opened.lock().unwrap(),
            vec!["https://example.invalid/browser-login".to_owned()]
        );
        assert!(io.output().contains("Opened auth URL with browser-cli."));
    });
}

#[test]
fn reports_browser_cli_open_failures_without_failing_the_login() {
    runtime().block_on(async {
        let provider_id = "test-cli-login-browser-failure";
        let storage = auth_storage();
        let model_runtime = create_runtime(storage).await;
        model_runtime.register_native_provider(oauth_provider(
            provider_id,
            "Test Browser CLI Failure",
            Box::new(|interaction| {
                Box::pin(async move {
                    interaction.notify(AuthEvent::AuthUrl {
                        url: "https://example.invalid/failing".to_owned(),
                        instructions: None,
                    });
                    Ok(OAuthCredential::new(
                        "refresh",
                        "access",
                        pi_ai::request::now_millis() as f64 + 60_000.0,
                    ))
                })
            }),
        ));

        let io = TestIo::new(&[]);
        let exit_code = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: provider_id.to_owned(),
            io: io.clone(),
            use_browser_cli: true,
            open_browser_cli_url: Some(Arc::new(|_url: String| {
                Box::pin(async { Err("browser-cli open failed with exit code 2".to_owned()) })
            })),
        })
        .await;

        assert_eq!(exit_code, 0);
        assert!(
            io.output()
                .contains("browser-cli open failed: browser-cli open failed with exit code 2")
        );
    });
}

#[test]
fn returns_nonzero_and_prints_provider_list_for_unknown_providers() {
    runtime().block_on(async {
        let storage = auth_storage();
        let model_runtime = create_runtime(storage.clone()).await;
        let io = TestIo::new(&[]);
        let exit_code = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: "missing-provider".to_owned(),
            io: io.clone(),
            use_browser_cli: false,
            open_browser_cli_url: None,
        })
        .await;

        assert_eq!(exit_code, 1);
        assert!(io.error().contains("Unknown OAuth provider: missing-provider"));
        assert!(io.error().contains("Available OAuth providers:"));
        assert!(
            storage
                .read("missing-provider", Default::default())
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn reports_login_failures_with_the_provider_name() {
    runtime().block_on(async {
        let provider_id = "test-cli-login-failure";
        let storage = auth_storage();
        let model_runtime = create_runtime(storage).await;
        model_runtime.register_native_provider(oauth_provider(
            provider_id,
            "Failing Provider",
            Box::new(|interaction| {
                Box::pin(async move {
                    interaction.notify(AuthEvent::Info {
                        message: "Starting".to_owned(),
                        links: Some(vec![AuthInfoLink {
                            url: "https://example.invalid/info".to_owned(),
                            label: Some("Docs".to_owned()),
                        }]),
                    });
                    Err(AuthError::failed("login rejected"))
                })
            }),
        ));

        let io = TestIo::new(&[]);
        let exit_code = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: provider_id.to_owned(),
            io: io.clone(),
            use_browser_cli: false,
            open_browser_cli_url: None,
        })
        .await;

        assert_eq!(exit_code, 1);
        assert!(io.output().contains("Starting"));
        assert!(io.output().contains("Docs: https://example.invalid/info"));
        assert!(
            io.error()
                .contains("Failed to login to Failing Provider: login rejected")
        );
    });
}

#[test]
fn prints_usage_for_help_and_unknown_flags() {
    runtime().block_on(async {
        let storage = auth_storage();
        let model_runtime = create_runtime(storage).await;
        for argv in [
            vec!["login", "--help"],
            vec!["login", "openai-codex", "--bogus"],
            vec!["login"],
        ] {
            let io = TestIo::new(&[]);
            let outcome = pi_coding_agent::cli::login_command::handle_login_command(
                &argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>(),
                LoginCommandDependencies {
                    model_runtime: Some(model_runtime.clone()),
                    io: Some(io.clone()),
                    use_browser_cli: None,
                    open_browser_cli_url: None,
                },
            )
            .await
            .unwrap();
            assert!(outcome.handled);
            let expected_exit = if argv.get(1).copied() == Some("--help") { 0 } else { 1 };
            assert_eq!(outcome.exit_code, Some(expected_exit));
            assert!(io.error().starts_with("Usage: pi login <provider> [--browser-cli]"));
        }
    });
}

#[test]
fn raw_oauth_error_reaches_captured_stderr_without_display_projection() {
    runtime().block_on(async {
        let model_runtime = create_runtime(auth_storage()).await;
        model_runtime.register_native_provider(oauth_provider(
            "raw-error",
            "Raw Provider",
            Box::new(|_| Box::pin(async { Err(AuthError::failed(JsString::from_utf16(vec![0xd800, 0x21, 0xdfff]))) })),
        ));
        let io = TestIo::new(&[]);
        let exit = run_login_command(RunLoginCommandOptions {
            model_runtime,
            provider_id: "raw-error".into(),
            io: io.clone(),
            use_browser_cli: false,
            open_browser_cli_url: None,
        })
        .await;
        assert_eq!(exit, 1);
        assert!(io.error().contains(&JsString::from_utf16(vec![0xd800, 0x21, 0xdfff])));
        assert!(!io.error().contains("�"));
    });
}

#[test]
fn default_stdio_io_is_constructible() {
    let _io = StdioLoginIo::new();
}
