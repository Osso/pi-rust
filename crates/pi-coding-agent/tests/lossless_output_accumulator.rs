use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[test]
fn spill_create_error_reaches_uncaught_process_boundary() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let locked = directory.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let path = locked.join("output.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env("PI_SPILL_FAILURE_PATH", &path)
        .args([
            "--ignored",
            "--exact",
            "spill_create_error_child",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "uncaught create failure must exit like Node, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        format!("Error: EACCES: permission denied, open '{}'", path.display())
    );
}

#[test]
fn spill_write_error_reaches_uncaught_process_boundary() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env("PI_SPILL_FAILURE_PATH", "/dev/full")
        .args([
            "--ignored",
            "--exact",
            "spill_create_error_child",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "Error: ENOSPC: no space left on device, write"
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "subprocess entry point"]
async fn spill_create_error_child() {
    let Some(path) = std::env::var_os("PI_SPILL_FAILURE_PATH") else {
        return;
    };
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_bytes: 1.0,
            ..OutputAccumulatorOptions::default()
        },
        path.to_string_lossy().into_owned(),
    );
    output.append(b"too large");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    panic!("expected unhandled stream failure");
}

#[test]
fn output_snapshot_keeps_raw_artifact_path_until_file_creation() {
    let directory = tempfile::tempdir().unwrap();
    let path =
        JsString::from(directory.path().to_str().unwrap()) + "/output-" + JsString::from_utf16(vec![0xd800]) + ".log";
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_bytes: 2.0,
            ..OutputAccumulatorOptions::default()
        },
        &path,
    );
    output.append(b"first\nlast\n");
    output.finish();
    let snapshot = output.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });
    output.close_temp_file().unwrap();
    assert_eq!(snapshot.full_output_path, Some(path.clone()));
    assert_eq!(std::fs::read(encode_node_utf8(&path)).unwrap(), b"first\nlast\n");
    assert_eq!(snapshot.content, snapshot.truncation.content);
}
