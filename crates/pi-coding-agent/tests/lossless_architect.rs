//! Architect observations keep lone UTF-16 units and serialize them as `JSON.stringify` does.

use pi_coding_agent::architect::observer::{
    ArchitectObservation, ArchitectObservationReason, ArchitectSessionSnapshot, create_architect_observation,
    snapshot_architect_sessions,
};
use pi_coding_agent::architect::prompt::build_architect_prompt;
use pi_js_compat::js_json::JsString;

fn session(id: Vec<u16>, cwd: Vec<u16>) -> ArchitectSessionSnapshot {
    ArchitectSessionSnapshot {
        cwd: JsString::from_utf16(cwd),
        goal_json: None,
        id: JsString::from_utf16(id),
        is_subagent: false,
        name: Some(JsString::from_utf16(vec![0x6e, 0xdc00])),
    }
}

#[test]
fn prompt_escapes_lone_surrogates_like_json_stringify() {
    let observation = ArchitectObservation {
        reason: ArchitectObservationReason::SessionStateChanged,
        requests: Vec::new(),
        sessions: vec![session(vec![0x61], vec![0x2f, 0x77, 0xd800])],
    };
    let prompt = build_architect_prompt(&observation);
    assert!(
        prompt.ends_with(
            r#"{"reason":"session_state_changed","requests":[],"sessions":[{"cwd":"/w\ud800","id":"a","isSubagent":false,"name":"n\udc00"}]}"#
        ),
        "{prompt}"
    );
}

#[test]
fn observation_change_detection_compares_raw_units() {
    let first = vec![session(vec![0x61], vec![0x2f, 0xd800])];
    let changed = vec![session(vec![0x61], vec![0x2f, 0xd801])];
    let previous = create_architect_observation(None, &first, &[]).unwrap();
    assert_eq!(create_architect_observation(Some(&previous), &first, &[]), None);
    let next = create_architect_observation(Some(&previous), &changed, &[]).unwrap();
    assert_eq!(next.sessions[0].cwd.units(), [0x2f, 0xd801]);
}

#[test]
fn sessions_sort_by_raw_unit_ids() {
    let sorted =
        snapshot_architect_sessions(&[session(vec![0x62], vec![0x2f]), session(vec![0x61, 0xd800], vec![0x2f])]);
    assert_eq!(sorted[0].id.units(), [0x61, 0xd800]);
}
