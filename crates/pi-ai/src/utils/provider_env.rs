use crate::types::ProviderEnv;

/// Resolve a provider env value from scoped overrides, then the process environment.
///
/// TypeScript falls back to `/proc/self/environ` only inside Bun compiled binaries
/// whose `process.env` is empty; native Rust reads the process environment directly,
/// so that Bun-specific fallback has no Rust equivalent (see traceability).
pub fn get_provider_env_value(name: &str, env: Option<&ProviderEnv>) -> Option<String> {
    resolve_provider_env_value(name, env, pi_js_compat::process_env::get)
}

/// Testable form of [`get_provider_env_value`] with an injected process environment.
///
/// JavaScript uses `||`, so empty strings fall through to the next source.
pub fn resolve_provider_env_value(
    name: &str,
    env: Option<&ProviderEnv>,
    process_env: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let scoped = env
        .and_then(|env| env.get(name))
        .filter(|value| !value.is_empty())
        .cloned();
    scoped.or_else(|| process_env(name).filter(|value| !value.is_empty()))
}
