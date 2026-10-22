//! Port of `packages/ai/src/auth/context.ts`.

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::sync::Arc;

use crate::request::BoxFuture;

use super::types::AuthContext;

/// Default auth context: process env vars and filesystem existence checks.
pub struct DefaultProviderAuthContext;

impl AuthContext for DefaultProviderAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<JsString>> {
        let name = name.to_owned();
        Box::pin(async move {
            pi_js_compat::process_env::get(&name)
                .filter(|value| !value.trim().is_empty())
                .map(JsString::from)
        })
    }

    fn file_exists(&self, path: &JsString) -> BoxFuture<'_, bool> {
        let path = path.to_owned();
        Box::pin(async move {
            let resolved = expand_home(&path);
            std::fs::metadata(encode_node_utf8(&resolved)).is_ok()
        })
    }
}

/// `defaultProviderAuthContext()`.
pub fn default_provider_auth_context() -> Arc<dyn AuthContext> {
    Arc::new(DefaultProviderAuthContext)
}

fn expand_home(path: &JsString) -> JsString {
    if !path.starts_with("~") {
        return path.clone();
    }
    match pi_js_compat::process_env::get("HOME") {
        Some(home) => js_format!("{}{}", home, path.slice(1, path.len())),
        None => path.clone(),
    }
}
