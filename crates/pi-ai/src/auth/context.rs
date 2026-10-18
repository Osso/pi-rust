//! Port of `packages/ai/src/auth/context.ts`.

use std::sync::Arc;

use crate::request::BoxFuture;

use super::types::AuthContext;

/// Default auth context: process env vars and filesystem existence checks.
pub struct DefaultProviderAuthContext;

impl AuthContext for DefaultProviderAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let name = name.to_owned();
        Box::pin(async move { std::env::var(name).ok().filter(|value| !value.trim().is_empty()) })
    }

    fn file_exists(&self, path: &str) -> BoxFuture<'_, bool> {
        let path = path.to_owned();
        Box::pin(async move {
            let resolved = expand_home(&path);
            std::fs::metadata(resolved).is_ok()
        })
    }
}

/// `defaultProviderAuthContext()`.
pub fn default_provider_auth_context() -> Arc<dyn AuthContext> {
    Arc::new(DefaultProviderAuthContext)
}

fn expand_home(path: &str) -> String {
    let Some(rest) = path.strip_prefix('~') else {
        return path.to_owned();
    };
    match std::env::var("HOME") {
        Ok(home) => format!("{home}{rest}"),
        Err(_) => path.to_owned(),
    }
}
