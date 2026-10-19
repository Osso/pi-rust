use pi_js_compat::process_env;

const CHILD_ENV: &str = "PI_PROCESS_ENV_NON_UNICODE_CHILD";
const PROBE_PREFIX: &str = "PI_PROCESS_ENV_PROBE_";
const PROBE_OUTPUT: &str = "PI_PROCESS_ENV_PROBE_OUTPUT:";

/// Child mode: print the lossy values `process_env` sees for the probe vars.
#[test]
fn non_unicode_environment_child() {
    if std::env::var(CHILD_ENV).is_err() {
        return;
    }
    let selected: serde_json::Map<String, serde_json::Value> = process_env::vars()
        .into_iter()
        .filter(|(name, _)| name.starts_with(PROBE_PREFIX))
        .map(|(name, value)| (name, serde_json::Value::String(value)))
        .collect();
    println!(
        "{PROBE_OUTPUT}{}",
        serde_json::to_string(&selected).expect("probe JSON")
    );
}

#[test]
fn decodes_non_unicode_environment_entries_like_node() {
    use std::os::unix::ffi::OsStringExt;

    let mut bad_key = std::ffi::OsString::from("PI_PROCESS_ENV_PROBE_BAD_KEY");
    bad_key.push(std::ffi::OsString::from_vec(vec![0xe0]));
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "non_unicode_environment_child", "--nocapture"])
        .env(CHILD_ENV, "1")
        .env(
            "PI_PROCESS_ENV_PROBE_BAD_VALUE",
            std::ffi::OsString::from_vec(b"a\xe0\xffz".to_vec()),
        )
        .env(
            "PI_PROCESS_ENV_PROBE_TRUNCATED",
            std::ffi::OsString::from_vec(b"x\xf0\x9fy".to_vec()),
        )
        .env(
            "PI_PROCESS_ENV_PROBE_OVERLONG",
            std::ffi::OsString::from_vec(b"x\xc0\xafy".to_vec()),
        )
        .env("PI_PROCESS_ENV_PROBE_VALID", "\u{1f600}")
        .env(&bad_key, "value")
        .output()
        .expect("non-Unicode child probe must run");
    assert!(
        output.status.success(),
        "child probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("child stdout must be UTF-8");
    let payload = stdout
        .lines()
        .find_map(|line| line.strip_prefix(PROBE_OUTPUT))
        .expect("child probe output");
    let values: serde_json::Value = serde_json::from_str(payload).expect("probe JSON");

    // Node replaces each invalid byte with U+FFFD and drops entries whose key is
    // not valid UTF-8 (verified against Node 26 `process.env`).
    assert_eq!(
        values,
        serde_json::json!({
            "PI_PROCESS_ENV_PROBE_BAD_VALUE": "a\u{fffd}\u{fffd}z",
            "PI_PROCESS_ENV_PROBE_TRUNCATED": "x\u{fffd}y",
            "PI_PROCESS_ENV_PROBE_OVERLONG": "x\u{fffd}\u{fffd}y",
            "PI_PROCESS_ENV_PROBE_VALID": "\u{1f600}",
        })
    );
}

#[test]
fn overrides_follow_node_process_env_semantics() {
    process_env::clear();
    let existing = std::env::vars().next().expect("a process environment variable").0;
    let original = std::env::var(&existing).ok();

    process_env::set("PI_PROCESS_ENV_TEST", "first");
    process_env::set("PI_PROCESS_ENV_SECOND", "second");
    process_env::set("PI_PROCESS_ENV_TEST", "updated");
    process_env::remove(&existing);

    assert_eq!(process_env::get("PI_PROCESS_ENV_TEST"), Some("updated".to_owned()));
    assert_eq!(process_env::get("PI_PROCESS_ENV_SECOND"), Some("second".to_owned()));
    assert_eq!(process_env::get(&existing), None);

    let vars = process_env::vars();
    assert!(vars.iter().all(|(name, _)| name != &existing));
    assert_eq!(
        vars.iter()
            .filter(|(name, _)| name.starts_with("PI_PROCESS_ENV_"))
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
        vec![("PI_PROCESS_ENV_TEST", "updated"), ("PI_PROCESS_ENV_SECOND", "second")]
    );

    process_env::remove("PI_PROCESS_ENV_TEST");
    process_env::remove("PI_PROCESS_ENV_SECOND");
    process_env::clear();
    if let Some(original) = original {
        assert_eq!(std::env::var(&existing).ok(), Some(original));
    }
}
