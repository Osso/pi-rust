use pi_coding_agent::core::detached_job_retention::{
    DetachedArtifactRetentionCandidate, DetachedArtifactRetentionPolicy, select_detached_artifact_directories_to_delete,
};
use pi_js_compat::js_json::JsString;

#[test]
fn retains_raw_selected_paths_and_validation_errors() {
    let path = JsString::from("/jobs/") + JsString::from_utf16(vec![0xd800]);
    let mut candidate = DetachedArtifactRetentionCandidate {
        directory_path: path.clone(),
        byte_size: 10.0,
        terminal_at: 1.0,
        protected_by_live_reference: false,
    };
    let policy = DetachedArtifactRetentionPolicy {
        now: 10.0,
        max_age: 1.0,
        max_bytes: 100.0,
    };
    assert_eq!(
        select_detached_artifact_directories_to_delete(&[candidate.clone()], &policy).unwrap(),
        vec![path.clone()]
    );
    candidate.byte_size = -1.0;
    assert_eq!(
        select_detached_artifact_directories_to_delete(&[candidate], &policy).unwrap_err(),
        JsString::from("Artifact ") + path + " byte size must be a nonnegative finite number"
    );
}
