//! Opt-in native `dynamic-resources/index.ts`.
//! The embedding host supplies this example's resource directory (the native
//! equivalent of `dirname(import.meta.url)`), not the session's cwd.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use std::path::Path;

pub fn dynamic_resources_extension(pi: &ExtensionApi, base_dir: &Path) -> Result<(), HandlerError> {
    let paths = ResourcesDiscoverResult {
        skill_paths: Some(vec![JsString::from(
            base_dir.join("SKILL.md").to_string_lossy().into_owned(),
        )]),
        prompt_paths: Some(vec![JsString::from(
            base_dir.join("dynamic.md").to_string_lossy().into_owned(),
        )]),
        theme_paths: Some(vec![JsString::from(
            base_dir.join("dynamic.json").to_string_lossy().into_owned(),
        )]),
    };
    pi.on_resources_discover(
        move |_: ResourcesDiscoverEvent,
              _: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ResourcesDiscoverResult>, HandlerError>> {
            let paths = paths.clone();
            Box::pin(async move { Ok(Some(paths)) })
        },
    )
    .map_err(HandlerError::new)
}
