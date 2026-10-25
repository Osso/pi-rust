use pi_coding_agent::core::agent_session_runtime::AgentSessionRuntime;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::{
    ExtensionFactory, LoadExtensionsResult, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::resource_loader as resources;
use pi_js_compat::js_json::{JsString, parse_lossless, stringify_lossless};

static RELOAD_ARMED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

static RELOAD_RUNTIME: OnceLock<AgentSessionRuntime> = OnceLock::new();

pub fn arm_reload(runtime: &AgentSessionRuntime) {
    if let Some(armed) = RELOAD_ARMED.get() {
        armed.store(true, Ordering::SeqCst);
        RELOAD_RUNTIME
            .set(runtime.clone())
            .unwrap_or_else(|_| panic!("one print runtime per fixture process"));
    }
}

pub async fn resource_loader(cwd: &JsString) -> Arc<dyn resources::ResourceLoader> {
    let Some(encoded) = std::env::var("PI_HARNESS_PRINT_FIXTURE").ok() else {
        return resources::StaticResourceLoader::new();
    };
    let config = parse_lossless(&encoded).unwrap();
    let probe = config.get("reloadProbe");
    let rebind = config
        .get("rebindProbe")
        .and_then(pi_js_compat::js_json::JsValue::as_bool)
        == Some(true);
    let early_signal = config
        .get("earlySignal")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .map(|signal| match signal {
            signal if signal == "SIGTERM" => rustix::process::Signal::TERM,
            signal if signal == "SIGHUP" => rustix::process::Signal::HUP,
            _ => panic!("unsupported fixture signal"),
        });
    let pending_shutdown = config
        .get("pendingShutdown")
        .and_then(pi_js_compat::js_json::JsValue::as_bool)
        == Some(true);
    if probe.is_none() && !rebind && early_signal.is_none() && !pending_shutdown {
        return resources::StaticResourceLoader::new();
    }
    let reload_enabled = probe.is_some();
    let root = PathBuf::from(std::env::var("PI_HARNESS_PRINT_ROOT").unwrap());
    let message = probe
        .map(|probe| probe.get("message").unwrap().as_string().unwrap().clone())
        .unwrap_or_default();
    let command_root = root.clone();
    let fifo = probe
        .and_then(|probe| probe.get("fifo"))
        .and_then(pi_js_compat::js_json::JsValue::as_bool)
        .unwrap_or(false);
    let success = probe
        .and_then(|probe| probe.get("success"))
        .and_then(pi_js_compat::js_json::JsValue::as_bool)
        .unwrap_or(false);
    let factory: ExtensionFactory = Arc::new(move |api| {
        if pending_shutdown {
            let root = command_root.clone();
            api.on_session_shutdown(
                move |_,
                      _|
                      -> BoxFuture<
                    'static,
                    Result<Option<()>, pi_coding_agent::core::extensions::types::HandlerError>,
                > {
                    std::fs::write(root.join("print-shutdown-ready"), "ready").unwrap();
                    Box::pin(std::future::pending())
                },
            )
            .unwrap();
        }
        api.register_command(
            "print-rebind-probe",
            None,
            None,
            Arc::new(|_, context| {
                Box::pin(async move {
                    context.new_session(None).await?;
                    Ok(())
                })
            }),
        )
        .unwrap();
        if let Some(signal) = early_signal {
            api.on_session_start(
                move |_,
                      _|
                      -> BoxFuture<
                    'static,
                    Result<Option<()>, pi_coding_agent::core::extensions::types::HandlerError>,
                > {
                    if !RELOAD_ARMED.get().unwrap().load(Ordering::SeqCst) {
                        return Box::pin(async { Ok(None) });
                    }
                    rustix::process::kill_process(
                        rustix::process::Pid::from_raw(std::process::id() as i32).unwrap(),
                        signal,
                    )
                    .unwrap();
                    Box::pin(std::future::pending())
                },
            )
            .unwrap();
        }
        let root = command_root.clone();
        api.register_command(
            "print-reload-probe",
            None,
            None,
            Arc::new(move |_args, context| {
                let root = root.clone();
                Box::pin(async move {
                    let first = fifo.then(|| {
                        let runtime = RELOAD_RUNTIME.get().unwrap().clone();
                        tokio::spawn(async move { runtime.reload(None).await })
                    });
                    if fifo {
                        while !root.join("print-reload-ready.json").exists() {
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        }
                    }
                    let reload = context.reload().await;
                    if let Some(first) = first {
                        assert!(first.await.unwrap().is_err());
                    }
                    let result = reload.and_then(|()| if success { context.is_idle().map(Some) } else { Ok(None) });
                    let outcome = match result {
                        Ok(Some(idle)) => pi_js_compat::js_json!({"returned":true,"oldContextIdle":idle}),
                        Ok(None) => pi_js_compat::js_json!({"returned":true}),
                        Err(error) => pi_js_compat::js_json!({"rejected":super::serialize_rejection(pi_js_compat::callback_error::CallbackError::from(error).thrown_value())}),
                    };
                    std::fs::write(root.join("print-reload-outcome.json"), stringify_lossless(&outcome)).unwrap();
                    Ok(())
                })
            }),
        )
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory,
        cwd,
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<print-fixture>".into(),
    )
    .await
    .unwrap();
    let inner = resources::StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: Vec::new(),
            runtime,
        }),
        ..Default::default()
    };
    let armed = Arc::new(AtomicBool::new(false));
    RELOAD_ARMED.set(Arc::clone(&armed)).unwrap();
    Arc::new(PrintResources {
        inner,
        root,
        message,
        armed,
        success,
        reload_enabled,
    })
}

struct PrintResources {
    inner: resources::StaticResourceLoader,
    root: PathBuf,
    message: JsString,
    armed: Arc<AtomicBool>,
    success: bool,
    reload_enabled: bool,
}

impl resources::ResourceLoader for PrintResources {
    fn get_extensions(&self) -> LoadExtensionsResult {
        let mut loaded = self.inner.get_extensions();
        loaded.runtime = create_extension_runtime();
        loaded
    }
    fn get_skills(&self) -> resources::LoadedSkills {
        self.inner.get_skills()
    }
    fn get_prompts(&self) -> resources::LoadedPrompts {
        self.inner.get_prompts()
    }
    fn get_themes(&self) -> resources::LoadedThemes {
        self.inner.get_themes()
    }
    fn get_agents_files(&self) -> resources::LoadedAgentsFiles {
        self.inner.get_agents_files()
    }
    fn get_rules_files(&self) -> resources::LoadedRulesFiles {
        self.inner.get_rules_files()
    }
    fn get_rules_content(&self) -> Option<JsString> {
        self.inner.get_rules_content()
    }
    fn get_system_prompt(&self) -> Option<JsString> {
        self.inner.get_system_prompt()
    }
    fn get_append_system_prompt(&self) -> Vec<JsString> {
        self.inner.get_append_system_prompt()
    }
    fn extend_resources(&self, paths: resources::ResourceExtensionPaths) -> Result<(), JsString> {
        self.inner.extend_resources(paths)
    }
    fn reload(&self) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async move {
            if !self.reload_enabled || !self.armed.load(Ordering::SeqCst) {
                return Ok(());
            }
            let mut entries = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.root.join("print-reload-entries.jsonl"))
                .unwrap();
            writeln!(entries, "{{\"entered\":true}}").unwrap();
            let ready = pi_js_compat::js_json!({"pid":std::process::id()});
            std::fs::write(self.root.join("print-reload-ready.pending"), stringify_lossless(&ready)).unwrap();
            std::fs::rename(
                self.root.join("print-reload-ready.pending"),
                self.root.join("print-reload-ready.json"),
            )
            .unwrap();
            while !self.root.join("print-reload-release").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            if self.success {
                Ok(())
            } else {
                Err(self.message.clone())
            }
        })
    }
    fn reload_with_options(
        &self,
        _resolve: Option<resources::ResolveProjectTrust>,
    ) -> BoxFuture<'_, Result<(), JsString>> {
        self.reload()
    }
}
