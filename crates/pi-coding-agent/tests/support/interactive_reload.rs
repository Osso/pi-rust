use crate::core::extensions::types::LoadExtensionsResult;
use crate::core::resource_loader::*;
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

pub(super) struct HeldReloadLoader {
    pub entered: tokio::sync::Notify,
    pub release: tokio::sync::Notify,
    pub error: Option<JsString>,
    inner: StaticResourceLoader,
}

impl HeldReloadLoader {
    pub fn new(error: Option<JsString>) -> Arc<Self> {
        Arc::new(Self {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            error,
            inner: StaticResourceLoader::default(),
        })
    }
}

impl ResourceLoader for HeldReloadLoader {
    fn get_extensions(&self) -> LoadExtensionsResult { self.inner.get_extensions() }
    fn get_skills(&self) -> LoadedSkills { self.inner.get_skills() }
    fn get_prompts(&self) -> LoadedPrompts { self.inner.get_prompts() }
    fn get_themes(&self) -> LoadedThemes { self.inner.get_themes() }
    fn get_agents_files(&self) -> LoadedAgentsFiles { self.inner.get_agents_files() }
    fn get_rules_files(&self) -> LoadedRulesFiles { self.inner.get_rules_files() }
    fn get_rules_content(&self) -> Option<JsString> { self.inner.get_rules_content() }
    fn get_system_prompt(&self) -> Option<JsString> { self.inner.get_system_prompt() }
    fn get_append_system_prompt(&self) -> Vec<JsString> { self.inner.get_append_system_prompt() }
    fn extend_resources(&self, paths: ResourceExtensionPaths) -> Result<(), JsString> { self.inner.extend_resources(paths) }
    fn reload(&self) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async {
            self.entered.notify_one();
            self.release.notified().await;
            match &self.error { Some(error) => Err(error.clone()), None => Ok(()) }
        })
    }
    fn reload_with_options(&self, _: Option<ResolveProjectTrust>) -> BoxFuture<'_, Result<(), JsString>> { self.reload() }
}
