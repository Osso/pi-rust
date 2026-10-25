//! Standalone: the real extension checks the process PATH at registration.
use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
use std::sync::Arc;

async fn register() -> pi_coding_agent::core::extensions::Extension {
    load_extension_from_factory(
        Arc::new(|pi| {
            let result =
                pi_coding_agent::bundled_extensions::browser_cli::browser_cli_extension(pi, Default::default());
            Box::pin(async move { result })
        }),
        &"/repo".into(),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &"<first-party:browser-cli>".into(),
    )
    .await
    .expect("extension")
}

#[tokio::test]
async fn browser_cli_registration_requires_an_executable_on_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("private PATH");
    let old_path = pi_js_compat::process_env::get("PATH");
    pi_js_compat::process_env::set("PATH", dir.path().to_str().unwrap());
    let absent = register().await;
    let executable = dir.path().join("browser-cli");
    std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let not_executable = register().await;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let available = register().await;
    match old_path {
        Some(path) => pi_js_compat::process_env::set("PATH", &path),
        None => pi_js_compat::process_env::remove("PATH"),
    }
    assert!(absent.read().tools.is_empty(), "absent browser-cli must not register");
    assert!(
        not_executable.read().tools.is_empty(),
        "non-executable browser-cli must not register"
    );
    assert!(available.read().tools.contains_key("browser-cli"));
}
