#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::footer_data_provider::FooterDataProvider;
use pi_js_compat::{
    abort::AbortReason,
    js_json::{JsString, JsValue, parse_lossless, stringify_lossless},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::Arc;

fn finish_group(child: std::process::Child) -> std::process::Output {
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("private group cleanup: {error}"),
    }
    output
}
#[test]
fn scheduled_branch_callback_throw_reaches_original_uncaught_authority_and_exit() {
    use std::os::unix::process::CommandExt;
    if let Ok(path) = std::env::var("PI_FOOTER_UNCAUGHT_PROBE") {
        pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
            let error = payload
                .downcast_ref::<pi_ai::request::CallbackError>()
                .expect("original callback error");
            let value = match error.thrown_value() {
                AbortReason::Error(error) => error.value(),
                _ => panic!("expected Error"),
            };
            std::fs::write(&path, stringify_lossless(&value)).unwrap();
        })));
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let cwd = std::env::var("PI_FOOTER_CHILD_CWD").unwrap();
                let provider = FooterDataProvider::new(cwd.as_str());
                provider.get_git_branch();
                let _subscription = provider.on_branch_change(|| {
                    Err(NativeNodeError::from_constructor(
                        NativeErrorClass::TypeError,
                        JsString::from("branch-") + JsString::from_utf16(vec![0xd800]),
                    )
                    .with_property("code", JsValue::String("EBRANCH".into()))
                    .into())
                });
                std::fs::write(format!("{cwd}/.git/HEAD"), "ref: refs/heads/next\n").unwrap();
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                std::process::exit(7);
            });
        return;
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    let head = root.path().join(".git/HEAD");
    std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let source = oracle::spawn_oracle_process(
        &lock,
        "footer-branch-uncaught-oracle.mts",
        "packages/coding-agent/src/core/footer-data-provider.ts",
        &[("PI_FOOTER_CHILD_CWD", root.path().into())],
        std::process::Stdio::piped(),
    );
    let source = finish_group(source);
    assert_eq!(
        source.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected = parse_lossless(String::from_utf8(source.stdout).unwrap().trim()).unwrap();
    std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
    let output_path = root.path().join("native.json");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "scheduled_branch_callback_throw_reaches_original_uncaught_authority_and_exit",
            "--nocapture",
        ])
        .env("PI_FOOTER_UNCAUGHT_PROBE", &output_path)
        .env("PI_FOOTER_CHILD_CWD", root.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let native = finish_group(child);
    assert_eq!(
        native.status.code(),
        source.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(
        parse_lossless(std::fs::read_to_string(output_path).unwrap()).unwrap(),
        expected
    );
}
