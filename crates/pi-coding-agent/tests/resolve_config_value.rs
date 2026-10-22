//! Ports `packages/coding-agent/test/resolve-config-value.test.ts`: literals, environment templates,
//! escapes, shell command execution, caching, and header resolution.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use pi_coding_agent::core::{
    clear_config_value_cache, resolve_config_value, resolve_config_value_or_throw, resolve_config_value_uncached,
    resolve_headers, resolve_headers_or_throw,
};

/// The command cache is process-wide, so command tests serialize.
static CACHE_LOCK: Mutex<()> = Mutex::new(());

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-resolve-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, JsString> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), JsString::from(*value)))
        .collect()
}

#[test]
fn resolves_literals_templates_and_escapes() {
    let env = map(&[("TEST_CONFIG_LEFT", "left"), ("TEST_CONFIG_RIGHT", "right")]);
    assert_eq!(
        resolve_config_value(&JsString::from("literal-key"), None).as_ref(),
        Some(&JsString::from("literal-key"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("$TEST_CONFIG_LEFT"), Some(&env)).as_ref(),
        Some(&JsString::from("left"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("${TEST_CONFIG_LEFT}_$TEST_CONFIG_RIGHT"), Some(&env)).as_ref(),
        Some(&JsString::from("left_right"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("$$TEST_CONFIG_LEFT"), Some(&env)).as_ref(),
        Some(&JsString::from("$TEST_CONFIG_LEFT"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("$!literal-$TEST_CONFIG_RIGHT"), Some(&env)).as_ref(),
        Some(&JsString::from("!literal-right"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("$TEST_CONFIG_MISSING"), Some(&env)),
        None
    );
}

#[test]
fn credential_scoped_environment_wins_over_the_process_environment() {
    let env = map(&[("TEST_CONFIG_SCOPED", "credential")]);
    assert_eq!(
        resolve_config_value(&JsString::from("$TEST_CONFIG_SCOPED"), Some(&env)).as_ref(),
        Some(&JsString::from("credential"))
    );
}

#[test]
fn executes_shell_commands_and_trims_their_output() {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_config_value_cache();
    assert_eq!(
        resolve_config_value(&JsString::from("!echo '  spaced-key  '"), None).as_ref(),
        Some(&JsString::from("spaced-key"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("!printf 'line1\\nline2'"), None).as_ref(),
        Some(&JsString::from("line1\nline2"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("!echo 'hello world' | tr ' ' '-'"), None).as_ref(),
        Some(&JsString::from("hello-world"))
    );
}

#[test]
fn failed_commands_resolve_to_none() {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_config_value_cache();
    for command in ["!exit 1", "!nonexistent-command-12345", "!printf ''"] {
        assert_eq!(
            resolve_config_value(&JsString::from(command), None),
            None,
            "command {command}"
        );
    }
}

#[test]
fn caches_successful_and_failed_commands_until_cleared() {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = temp_dir("cache");
    let counter = dir.join("counter");
    fs::write(&counter, "0").unwrap();
    let counter_path = counter.display();
    let success =
        format!("!sh -c 'count=$(cat \"{counter_path}\"); echo $((count + 1)) > \"{counter_path}\"; echo value'");

    clear_config_value_cache();
    assert_eq!(
        resolve_config_value(&JsString::from(&success), None).as_ref(),
        Some(&JsString::from("value"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from(&success), None).as_ref(),
        Some(&JsString::from("value"))
    );
    assert_eq!(fs::read_to_string(&counter).unwrap().trim(), "1");

    clear_config_value_cache();
    assert_eq!(
        resolve_config_value(&JsString::from(&success), None).as_ref(),
        Some(&JsString::from("value"))
    );
    assert_eq!(fs::read_to_string(&counter).unwrap().trim(), "2");

    let failure = format!("!sh -c 'count=$(cat \"{counter_path}\"); echo $((count + 1)) > \"{counter_path}\"; exit 1'");
    assert_eq!(resolve_config_value(&JsString::from(&failure), None), None);
    assert_eq!(resolve_config_value(&JsString::from(&failure), None), None);
    assert_eq!(fs::read_to_string(&counter).unwrap().trim(), "3");
}

#[test]
fn uncached_resolution_executes_a_command_on_every_call() {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = temp_dir("uncached");
    let counter = dir.join("counter");
    fs::write(&counter, "0").unwrap();
    let counter_path = counter.display();
    let command =
        format!("!sh -c 'count=$(cat \"{counter_path}\"); echo $((count + 1)) > \"{counter_path}\"; echo value'");
    clear_config_value_cache();
    assert_eq!(
        resolve_config_value_uncached(&JsString::from(&command), None).as_ref(),
        Some(&JsString::from("value"))
    );
    assert_eq!(
        resolve_config_value_uncached(&JsString::from(&command), None).as_ref(),
        Some(&JsString::from("value"))
    );
    assert_eq!(fs::read_to_string(&counter).unwrap().trim(), "2");
}

#[test]
fn environment_values_are_never_cached() {
    let env_first = map(&[("TEST_CONFIG_DYNAMIC", "first")]);
    let env_second = map(&[("TEST_CONFIG_DYNAMIC", "second")]);
    assert_eq!(
        resolve_config_value(&JsString::from("$TEST_CONFIG_DYNAMIC"), Some(&env_first)).as_ref(),
        Some(&JsString::from("first"))
    );
    assert_eq!(
        resolve_config_value(&JsString::from("$TEST_CONFIG_DYNAMIC"), Some(&env_second)).as_ref(),
        Some(&JsString::from("second"))
    );
}

#[test]
fn throws_name_the_failing_source() {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_config_value_cache();
    let error =
        resolve_config_value_or_throw(&JsString::from("!exit 1"), &JsString::from("API key"), None).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Failed to resolve API key from shell command: exit 1"
    );

    let env = map(&[("PRESENT", "value")]);
    let error = resolve_config_value_or_throw(&JsString::from("$MISSING_ONE"), &JsString::from("API key"), Some(&env))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Failed to resolve API key from environment variable: MISSING_ONE"
    );

    let error = resolve_config_value_or_throw(
        &JsString::from("$MISSING_ONE-$MISSING_TWO"),
        &JsString::from("API key"),
        Some(&env),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Failed to resolve API key from environment variables: MISSING_ONE, MISSING_TWO"
    );

    assert_eq!(
        resolve_config_value_or_throw(&JsString::from("$PRESENT"), &JsString::from("API key"), Some(&env)).unwrap(),
        "value"
    );
}

#[test]
fn headers_resolve_like_values_and_drop_empty_results() {
    let env = map(&[("TOKEN", "secret")]);
    let headers = map(&[
        ("Authorization", "Bearer $TOKEN"),
        ("X-Literal", "plain"),
        ("X-Missing", "$ABSENT"),
    ]);
    let resolved = resolve_headers(Some(&headers), Some(&env)).expect("some headers resolve");
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved["Authorization"], "Bearer secret");
    assert_eq!(resolved["X-Literal"], "plain");

    assert_eq!(resolve_headers(None, Some(&env)), None);
    let empty = map(&[("X-Missing", "$ABSENT")]);
    assert_eq!(resolve_headers(Some(&empty), Some(&env)), None);
}

#[test]
fn header_errors_name_the_header() {
    let env = map(&[("PRESENT", "value")]);
    let headers = map(&[("Authorization", "Bearer $ABSENT")]);
    let error = resolve_headers_or_throw(Some(&headers), &JsString::from("Provider"), Some(&env)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Failed to resolve Provider header \"Authorization\" from environment variable: ABSENT"
    );

    let resolved = resolve_headers_or_throw(
        Some(&map(&[("X-Key", "$PRESENT")])),
        &JsString::from("Provider"),
        Some(&env),
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolved["X-Key"], "value");
    assert_eq!(
        resolve_headers_or_throw(None, &JsString::from("Provider"), Some(&env)).unwrap(),
        None
    );
}
