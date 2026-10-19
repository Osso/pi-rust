use pi_js_compat::process_env;

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
