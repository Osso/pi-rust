use std::collections::BTreeMap;

use pi_ai::types::ProviderEnv;
use pi_ai::utils::provider_env::{get_provider_env_value, resolve_provider_env_value};

fn env(pairs: &[(&str, &str)]) -> ProviderEnv {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn process_env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn scoped_override_wins_over_process_env() {
    let scoped = env(&[("API_KEY", "scoped")]);
    let process = process_env(&[("API_KEY", "process")]);
    let value = resolve_provider_env_value("API_KEY", Some(&scoped), |name| process.get(name).cloned());
    assert_eq!(value.as_ref(), Some(&pi_js_compat::js_json::JsString::from("scoped")));
}

#[test]
fn empty_scoped_override_falls_through_to_process_env() {
    let scoped = env(&[("API_KEY", "")]);
    let process = process_env(&[("API_KEY", "process")]);
    let value = resolve_provider_env_value("API_KEY", Some(&scoped), |name| process.get(name).cloned());
    assert_eq!(value.as_ref(), Some(&pi_js_compat::js_json::JsString::from("process")));
}

#[test]
fn missing_everywhere_is_none() {
    let value = resolve_provider_env_value("API_KEY", None, |_| None);
    assert_eq!(value, None);
}

#[test]
fn reads_the_real_process_environment_when_unscoped() {
    // `PATH` is present in every supported test runner; the exact value is irrelevant.
    let value = get_provider_env_value("PATH", None);
    assert!(value.is_some_and(|path| !path.is_empty()));
}
