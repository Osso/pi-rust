use super::{agent_environment, fixture, pty};
use pi_coding_agent::core::{
    agent_session_runtime::{
        CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeResult, create_agent_session_runtime,
    },
    agent_session_services::AgentSessionServices,
    auth_storage::AuthStorage,
    extensions::types::LoadExtensionsResult,
    model_registry::ModelRegistry,
    resource_loader::*,
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions},
};
use pi_coding_agent::modes::interactive::interactive_mode::{InteractiveMode, InteractiveModeOptions};
use pi_js_compat::js_json::JsString;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

struct FailAtResourceDisplay {
    inner: StaticResourceLoader,
    reloaded: AtomicBool,
    marker: PathBuf,
}
impl ResourceLoader for FailAtResourceDisplay {
    fn get_extensions(&self) -> LoadExtensionsResult {
        self.inner.get_extensions()
    }
    fn get_skills(&self) -> LoadedSkills {
        self.inner.get_skills()
    }
    fn get_prompts(&self) -> LoadedPrompts {
        self.inner.get_prompts()
    }
    fn get_themes(&self) -> LoadedThemes {
        self.inner.get_themes()
    }
    fn get_agents_files(&self) -> LoadedAgentsFiles {
        self.inner.get_agents_files()
    }
    fn get_rules_files(&self) -> LoadedRulesFiles {
        if self.reloaded.load(Ordering::SeqCst) {
            std::fs::write(&self.marker, "fail").unwrap();
        }
        LoadedRulesFiles {
            rules_files: vec![RuleFile {
                path: "/outside/rules.md".into(),
                content: "fixture rule".into(),
            }],
        }
    }
    fn get_rules_content(&self) -> Option<JsString> {
        None
    }
    fn get_system_prompt(&self) -> Option<JsString> {
        self.inner.get_system_prompt()
    }
    fn get_append_system_prompt(&self) -> Vec<JsString> {
        self.inner.get_append_system_prompt()
    }
    fn extend_resources(&self, paths: ResourceExtensionPaths) -> Result<(), JsString> {
        self.inner.extend_resources(paths)
    }
    fn reload(&self) -> pi_ai::request::BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async {
            self.reloaded.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
    fn reload_with_options(
        &self,
        _: Option<ResolveProjectTrust>,
    ) -> pi_ai::request::BoxFuture<'_, Result<(), JsString>> {
        self.reload()
    }
}

#[test]
fn reload_catches_resource_display_homedir_error_without_crashing() {
    let fixture = fixture();
    let library = fixture._temp.path().join("passwd-failure.so");
    let marker = fixture._temp.path().join("fail-resource-home");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let output = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut environment = agent_environment(&fixture);
    environment.extend([
        ("LD_PRELOAD".to_owned(), library.to_str().unwrap().to_owned()),
        (
            "PI_TEST_PASSWD_FAILURE_FILE".to_owned(),
            marker.to_str().unwrap().to_owned(),
        ),
        (
            "PI_TEST_PASSWD_HOME".to_owned(),
            fixture.home.to_str().unwrap().to_owned(),
        ),
    ]);
    let executable = std::env::current_exe().unwrap();
    let mut child = pty::spawn(
        Path::new("/usr/bin/env"),
        &[
            "-u",
            "HOME",
            executable.to_str().unwrap(),
            "--ignored",
            "--exact",
            "resource_home::reload_home_child",
            "--nocapture",
        ],
        &environment,
        &fixture.work,
    );
    child.wait_for(
        "to show full startup help and loaded resources.",
        Duration::from_secs(20),
    );
    child.write_input("/reload\r");
    child.wait_for(
        "Reload failed: A system error occurred: uv_os_homedir returned EIO (i/o error)",
        Duration::from_secs(20),
    );
    assert!(marker.exists(), "display getter must have enabled the NSS fault");
    assert!(!child.output().contains("pi exiting due to uncaughtException:"));
    child.write_input("\x03\x03");
    assert_eq!(child.wait(Duration::from_secs(20)), 0);
}

#[test]
#[ignore = "PTY child entry point"]
fn reload_home_child() {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(tokio::task::LocalSet::new().run_until(async {
            let cwd: JsString = std::env::current_dir().unwrap().to_str().unwrap().into();
            let agent_dir: JsString = std::env::var("PI_CODING_AGENT_DIR").unwrap().into();
            let loader: Arc<dyn ResourceLoader> = Arc::new(FailAtResourceDisplay {
                inner: StaticResourceLoader::default(),
                reloaded: AtomicBool::new(false),
                marker: std::env::var("PI_TEST_PASSWD_FAILURE_FILE").unwrap().into(),
            });
            let faux = pi_ai::compat::register_faux_provider(Default::default());
            let model = faux.get_model(None).unwrap();
            let auth_storage = AuthStorage::in_memory([(
                model.provider.clone(),
                serde_json::json!({"type":"api_key","key":"fixture"}),
            )]);
            let registry = ModelRegistry::in_memory(auth_storage.clone()).unwrap();
            let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
                Settings::new(),
                SettingsManagerCreateOptions::default(),
            )));
            let factory: CreateAgentSessionRuntimeFactory = Arc::new(move |request| {
                let loader = loader.clone();
                let registry = registry.clone();
                let auth_storage = auth_storage.clone();
                let model = model.clone();
                let settings = settings.clone();
                Box::pin(async move {
                    let result = create_agent_session(CreateAgentSessionOptions {
                        cwd: Some(request.cwd.clone()),
                        agent_dir: Some(request.agent_dir.clone()),
                        session_manager: Some(request.session_manager),
                        resource_loader: Some(loader.clone()),
                        settings_manager: Some(settings.clone()),
                        model_registry: Some(registry.clone()),
                        auth_storage: Some(auth_storage.clone()),
                        model: Some(model),
                        ..Default::default()
                    })
                    .await
                    .map_err(|error| error.message)?;
                    Ok(CreateAgentSessionRuntimeResult {
                        session: result.session,
                        services: AgentSessionServices {
                            cwd: request.cwd,
                            agent_dir: request.agent_dir,
                            model_registry: registry,
                            auth_storage,
                            settings_manager: settings,
                            resource_loader: loader,
                            diagnostics: vec![],
                        },
                        diagnostics: vec![],
                        model_fallback_message: None,
                    })
                })
            });
            let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
            let runtime = create_agent_session_runtime(factory, &cwd, &agent_dir, manager, None, None)
                .await
                .unwrap();
            let mode = InteractiveMode::new(runtime, InteractiveModeOptions::default(), None)
                .await
                .unwrap();
            mode.run().await.unwrap();
        }));
}
