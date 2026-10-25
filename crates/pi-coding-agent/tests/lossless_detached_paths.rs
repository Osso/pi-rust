use crate::shared_ai_oracle as oracle;
use pi_coding_agent::core::detached_job_runner::{create_detached_job_artifacts, reserve_detached_job_artifacts};
use pi_js_compat::js_json::{JsString, encode_node_utf8, to_value};

#[test]
fn detached_manifest_keeps_raw_paths_env_keys_and_file_bytes() {
    use pi_coding_agent::core::detached_bash_runner::{
        DetachedBashLaunchManifestData, write_detached_bash_launch_manifest,
    };
    use pi_coding_agent::core::session_control_db::RuntimeMailboxAddress;
    let directory = tempfile::tempdir().unwrap();
    let root = JsString::from(directory.path().to_str().unwrap());
    let raw = JsString::from_utf16(vec![0xd800]);
    let artifacts = create_detached_job_artifacts(&root, JsString::from("job-") + &raw).unwrap();
    let path = artifacts.directory.clone() + "/launch-" + &raw + ".json";
    let env = pi_js_compat::js_json::parse_lossless(r#"{"ENV_\udfff":"value-\ud801"}"#)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    let data = DetachedBashLaunchManifestData {
        activation_path: artifacts.directory.clone() + "/activation-" + &raw,
        args: vec![JsString::from("arg-") + &raw],
        command: JsString::from("/shell-") + &raw,
        control_db_path: root.clone() + "/control-" + &raw,
        cwd: root.clone() + "/cwd-" + &raw,
        env,
        foreground_completion_path: artifacts.directory.clone() + "/done-" + &raw,
        runner_address: RuntimeMailboxAddress {
            session_id: JsString::from("session-") + &raw,
            agent_id: Some(JsString::from("agent-") + &raw),
        },
        session_path: root + "/session-" + &raw,
        timeout_ms: None,
        tool_call_id: Some(JsString::from("tool-") + &raw),
        artifacts,
    };
    write_detached_bash_launch_manifest(&path, &data).unwrap();
    let actual = std::fs::read(encode_node_utf8(&path)).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/detached-bash-runner.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"detachedManifest","path":path,"data":data}));
    assert_eq!(
        JsString::from(String::from_utf8(actual).unwrap()),
        expected["wire"].as_string().unwrap().clone()
    );
}

#[test]
fn distinct_raw_output_paths_reject_before_filesystem_or_spawn() {
    use pi_coding_agent::core::detached_job_bootstrap::{GatedDetachedPayloadInput, spawn_gated_detached_payload};
    let cwd = JsString::from("/unused");
    let stdout = JsString::from("/unused/output-") + JsString::from_utf16(vec![0xd800]);
    let stderr = JsString::from("/unused/output-") + JsString::from_utf16(vec![0xd801]);
    let identity = JsString::from("/unused/identity.json");
    let command = JsString::from("/bin/true");
    let error = spawn_gated_detached_payload(&GatedDetachedPayloadInput {
        args: &[],
        command: &command,
        cwd: &cwd,
        env: None,
        identity_path: &identity,
        stderr_path: &stderr,
        stdout_path: &stdout,
    })
    .unwrap_err();
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/detached-job-bootstrap.ts",
    );
    let expected=source.request(&pi_js_compat::js_json!({"kind":"bootstrapSplit","input":{"args":[],"command":command,"cwd":cwd,"identityPath":identity,"stderrPath":stderr,"stdoutPath":stdout}}));
    assert_eq!(&error.message(), expected["error"].as_string().unwrap());
}

#[test]
fn detached_artifact_paths_and_collision_errors_match_frozen_units() {
    let directory = tempfile::tempdir().unwrap();
    let root = JsString::from(directory.path().to_str().unwrap()) + "/root-" + JsString::from_utf16(vec![0xd800]);
    let job = JsString::from("job-") + JsString::from_utf16(vec![0xdfff]);
    let actual = create_detached_job_artifacts(&root, &job).unwrap();
    assert!(std::path::Path::new(&encode_node_utf8(&actual.directory)).is_dir());
    let mut source = oracle::TsOracle::spawn(
        "lossless-tool-paths-oracle.ts",
        "packages/coding-agent/src/core/detached-job-runner.ts",
    );
    let expected=source.request(&pi_js_compat::js_json!({"kind":"detachedArtifacts","operation":"createDetachedJobArtifacts","root":root,"job":job}));
    assert_eq!(to_value(&actual).unwrap(), expected["artifacts"]);
    let collision = JsString::from("job-") + JsString::from_utf16(vec![0xd800]);
    let error = reserve_detached_job_artifacts(&root, &collision).unwrap_err();
    let expected=source.request(&pi_js_compat::js_json!({"kind":"detachedArtifacts","operation":"reserveDetachedJobArtifacts","root":root,"job":collision}));
    assert_eq!(&error.message(), expected["error"].as_string().unwrap());
    let invalid=source.request(&serde_json::json!({"kind":"detachedArtifacts","operation":"createDetachedJobArtifacts","root":"unused","job":""}));
    assert_eq!(invalid["error"], "Detached job ID must be one path segment");
}
