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

pub fn arm_reload() {
    if let Some(armed) = RELOAD_ARMED.get() {
        armed.store(true, Ordering::SeqCst);
    }
}

pub async fn resource_loader(cwd: &JsString) -> Arc<dyn resources::ResourceLoader> {
    let Some(encoded) = std::env::var("PI_HARNESS_PRINT_FIXTURE").ok() else {
        return resources::StaticResourceLoader::new();
    };
    let config = parse_lossless(&encoded).unwrap();
    let Some(probe) = config.get("reloadProbe") else {
        return resources::StaticResourceLoader::new();
    };
    let root = PathBuf::from(std::env::var("PI_HARNESS_PRINT_ROOT").unwrap());
    let message = probe.get("message").unwrap().as_string().unwrap().clone();
    let command_root = root.clone();
    let factory: ExtensionFactory = Arc::new(move |api| {
        let root = command_root.clone();
        api.register_command(
            "print-reload-probe",
            None,
            None,
            Arc::new(move |_args, context| {
                let root = root.clone();
                Box::pin(async move {
                    context.reload().await;
                    let outcome = pi_js_compat::js_json!({"returned":true});
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
    })
}

struct PrintResources {
    inner: resources::StaticResourceLoader,
    root: PathBuf,
    message: JsString,
    armed: Arc<AtomicBool>,
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
    fn get_system_prompt_source(&self) -> Option<resources::LoadedPathSource> {
        self.inner.get_system_prompt_source()
    }
    fn get_append_system_prompt(&self) -> Vec<JsString> {
        self.inner.get_append_system_prompt()
    }
    fn get_append_system_prompt_sources(&self) -> Vec<resources::LoadedPathSource> {
        self.inner.get_append_system_prompt_sources()
    }
    fn extend_resources(&self, paths: resources::ResourceExtensionPaths) -> Result<(), JsString> {
        self.inner.extend_resources(paths)
    }
    fn reload(&self) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async move {
            if !self.armed.load(Ordering::SeqCst) {
                return Ok(());
            }
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
            Err(self.message.clone())
        })
    }
    fn reload_with_options(
        &self,
        _resolve: Option<resources::ResolveProjectTrust>,
    ) -> BoxFuture<'_, Result<(), JsString>> {
        self.reload()
    }
}
