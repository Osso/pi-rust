use pi_coding_agent::core::detached_job_retention::{
    DetachedArtifactRetentionCandidate, DetachedArtifactRetentionPolicy, select_detached_artifact_directories_to_delete,
};

const NOW: f64 = 10_000.0;

fn candidate(
    directory_path: &str,
    byte_size: f64,
    terminal_at: f64,
    protected_by_live_reference: bool,
) -> DetachedArtifactRetentionCandidate {
    DetachedArtifactRetentionCandidate {
        directory_path: directory_path.into(),
        byte_size,
        terminal_at,
        protected_by_live_reference,
    }
}

fn policy(max_age: f64, max_bytes: f64) -> DetachedArtifactRetentionPolicy {
    DetachedArtifactRetentionPolicy {
        now: NOW,
        max_age,
        max_bytes,
    }
}

#[test]
fn selects_every_unprotected_artifact_at_or_beyond_the_maximum_age() {
    let candidates = [
        candidate("/jobs/recent", 10.0, 9_001.0, false),
        candidate("/jobs/boundary", 20.0, 9_000.0, false),
        candidate("/jobs/old", 30.0, 8_000.0, false),
        candidate("/jobs/protected-old", 40.0, 8_000.0, true),
    ];
    assert_eq!(
        select_detached_artifact_directories_to_delete(&candidates, &policy(1_000.0, 1_000.0)).unwrap(),
        vec!["/jobs/old", "/jobs/boundary"]
    );
}

#[test]
fn then_selects_oldest_unprotected_artifacts_until_retained_bytes_reach_the_cap() {
    let candidates = [
        candidate("/jobs/newest", 30.0, 9_900.0, false),
        candidate("/jobs/oldest", 30.0, 9_700.0, false),
        candidate("/jobs/middle", 30.0, 9_800.0, false),
        candidate("/jobs/protected", 50.0, 9_600.0, true),
    ];
    assert_eq!(
        select_detached_artifact_directories_to_delete(&candidates, &policy(10_000.0, 90.0)).unwrap(),
        vec!["/jobs/oldest", "/jobs/middle"]
    );
}

#[test]
fn uses_the_directory_path_as_a_deterministic_tie_break_for_equal_timestamps() {
    let candidates = [
        candidate("/jobs/z", 10.0, 9_500.0, false),
        candidate("/jobs/a", 10.0, 9_500.0, false),
    ];
    assert_eq!(
        select_detached_artifact_directories_to_delete(&candidates, &policy(10_000.0, 10.0)).unwrap(),
        vec!["/jobs/a"]
    );
}

#[test]
fn returns_no_protected_paths_even_when_protected_bytes_exceed_the_cap() {
    let candidates = [
        candidate("/jobs/protected", 200.0, 0.0, true),
        candidate("/jobs/keep", 10.0, 9_900.0, false),
    ];
    assert_eq!(
        select_detached_artifact_directories_to_delete(&candidates, &policy(1_000.0, 100.0)).unwrap(),
        vec!["/jobs/keep"]
    );
}

#[test]
fn keeps_everything_when_within_the_cap_and_age() {
    let candidates = [candidate("/jobs/a", 10.0, 9_999.0, false)];
    assert!(
        select_detached_artifact_directories_to_delete(&candidates, &policy(10_000.0, 100.0))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rejects_invalid_nonnegative_finite_values() {
    let cases: [(
        &str,
        Vec<DetachedArtifactRetentionCandidate>,
        DetachedArtifactRetentionPolicy,
    ); 4] = [
        (
            "Artifact /jobs/a byte size",
            vec![candidate("/jobs/a", f64::NAN, 0.0, false)],
            policy(1.0, 1.0),
        ),
        (
            "Retention policy now",
            Vec::new(),
            DetachedArtifactRetentionPolicy {
                now: -1.0,
                max_age: 1.0,
                max_bytes: 1.0,
            },
        ),
        (
            "Retention policy maximum age",
            Vec::new(),
            DetachedArtifactRetentionPolicy {
                now: NOW,
                max_age: f64::INFINITY,
                max_bytes: 1.0,
            },
        ),
        (
            "Retention policy maximum bytes",
            Vec::new(),
            DetachedArtifactRetentionPolicy {
                now: NOW,
                max_age: 1.0,
                max_bytes: -1.0,
            },
        ),
    ];
    for (message, candidates, policy) in cases {
        let error = select_detached_artifact_directories_to_delete(&candidates, &policy)
            .expect_err("invalid input must be rejected");
        assert!(
            error.starts_with(message) && error.ends_with("must be a nonnegative finite number"),
            "{error:?}"
        );
    }
}

#[test]
fn rejects_a_non_finite_terminal_timestamp() {
    let candidates = [candidate("/jobs/a", 1.0, f64::NEG_INFINITY, false)];
    let error = select_detached_artifact_directories_to_delete(&candidates, &policy(1.0, 1.0)).unwrap_err();
    assert_eq!(
        error,
        "Artifact /jobs/a terminal timestamp must be a nonnegative finite number"
    );
}
