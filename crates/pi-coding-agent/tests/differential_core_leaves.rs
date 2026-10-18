//! Differential tests: the pinned TypeScript core leaf modules versus the Rust port.

#[path = "support/oracle.rs"]
mod oracle;

use std::fs;

use pi_coding_agent::cli::experimental::auth::{RawAuthOptions, parse_auth_input};
use pi_coding_agent::core::permissions::policy::{
    APPROVAL_POLICIES, ApprovalPolicyContext, evaluate_approval_policy, is_approval_policy, normalize_approval_policy,
};
use pi_coding_agent::core::permissions::presets::{
    APPROVAL_PRESETS, SANDBOX_PROFILES, approval_preset_for_policy, approval_preset_to_bypass_permissions,
    find_approval_preset, is_approval_preset_name, is_sandbox_profile_name,
};
use pi_coding_agent::core::pi_manifest::read_pi_manifest;
use pi_coding_agent::core::provider_display_names::BUILT_IN_PROVIDER_DISPLAY_NAMES;
use pi_coding_agent::core::radius::RADIUS_PROVIDER_ID;
use pi_coding_agent::core::session_errors::SessionImportFileNotFoundError;
use pi_coding_agent::core::session_health::{
    SessionCheckStatus, SessionHealthRecord, SessionLiveStatus, advance_generation_for_pid, derive_live_status,
    empty_session_health, end_session_health, is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use pi_coding_agent::core::tool_capabilities::{CHILD_DISABLED_AGENT_TOOL_NAMES, SUPERVISOR_ONLY_TOOL_NAMES};
use serde_json::{Value, json};

const NAME_CASES: [&str; 12] = [
    "on-request",
    "never",
    "auto-approve",
    "always",
    "",
    "Never",
    " never",
    "ask-me",
    "auto-approve ",
    "read-only",
    "workspace-write",
    "full-access",
];

struct Oracle {
    inner: oracle::TsOracle,
    next_id: u64,
}

impl Oracle {
    fn spawn() -> Self {
        Self {
            inner: oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src"),
            next_id: 27_000,
        }
    }

    fn call(&mut self, module: &str, operation: &str, cases: Value) -> Value {
        self.call_raw(json!({"module": module, "operation": operation, "cases": cases}))
    }

    fn call_raw(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        request["id"] = json!(self.next_id);
        let response = self.inner.request(&request);
        assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
        response["output"].clone()
    }
}

#[test]
fn approval_policy_matches_typescript() {
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/permissions/policy.ts", "policy", json!(NAME_CASES));
    let actual: Vec<Value> = NAME_CASES
        .iter()
        .map(|value| {
            json!({
                "isApprovalPolicy": is_approval_policy(value),
                "normalized": normalize_approval_policy(Some(value)),
            })
        })
        .collect();
    assert_eq!(json!(actual), expected);

    let expected = oracle.call("core/permissions/policy.ts", "policyEvaluate", json!([]));
    let actual: Vec<Value> = APPROVAL_POLICIES
        .iter()
        .flat_map(|policy| {
            [true, false].map(|approval_required| {
                json!(evaluate_approval_policy(
                    *policy,
                    ApprovalPolicyContext { approval_required }
                ))
            })
        })
        .collect();
    assert_eq!(json!(actual), expected);
}

#[test]
fn approval_presets_match_typescript() {
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/permissions/presets.ts", "presets", json!(NAME_CASES));
    let actual = json!({
        "approvalPresets": APPROVAL_PRESETS,
        "sandboxProfiles": SANDBOX_PROFILES,
        "found": APPROVAL_PRESETS.iter().map(|preset| find_approval_preset(preset.name)).collect::<Vec<_>>(),
        "forPolicy": APPROVAL_POLICIES.map(approval_preset_for_policy),
        "bypass": APPROVAL_PRESETS.map(|preset| approval_preset_to_bypass_permissions(preset.name)),
        "names": NAME_CASES.map(|value| json!({
            "preset": is_approval_preset_name(value),
            "sandbox": is_sandbox_profile_name(value),
        })),
    });
    assert_eq!(actual, expected);
}

#[test]
fn constant_modules_match_typescript() {
    let mut oracle = Oracle::spawn();
    assert_eq!(
        oracle.call("core/provider-display-names.ts", "providerDisplayNames", json!([])),
        json!(BUILT_IN_PROVIDER_DISPLAY_NAMES)
    );
    assert_eq!(
        oracle.call("core/radius.ts", "radius", json!([])),
        json!(RADIUS_PROVIDER_ID)
    );
    assert_eq!(
        oracle.call("core/tool-capabilities.ts", "toolCapabilities", json!([])),
        json!({"child": CHILD_DISABLED_AGENT_TOOL_NAMES, "supervisor": SUPERVISOR_ONLY_TOOL_NAMES})
    );
    let paths = ["/tmp/missing.jsonl", "", "relative/ünïcode.jsonl"];
    let actual: Vec<Value> = paths
        .iter()
        .map(|path| {
            let error = SessionImportFileNotFoundError::new(*path);
            json!({"name": SessionImportFileNotFoundError::NAME, "message": error.to_string(), "filePath": error.file_path})
        })
        .collect();
    assert_eq!(
        oracle.call("core/session-errors.ts", "sessionErrors", json!(paths)),
        json!(actual)
    );
}

const MANIFEST_FIXTURES: [&[u8]; 14] = [
    br#"{"pi":{"extensions":["a.ts"],"skills":["s"],"prompts":["p"],"themes":["t"]}}"#,
    br#"{"pi":{"extensions":["a.ts",1],"skills":[],"prompts":null,"themes":{}}}"#,
    br#"{"pi":{}}"#,
    br#"{"pi":[]}"#,
    br#"{"pi":null}"#,
    br#"{"pi":"x"}"#,
    br#"{"name":"pkg"}"#,
    br#"[1,2]"#,
    b"null",
    b"{bad json",
    b"",
    b"\xEF\xBB\xBF{\"pi\":{}}",
    b"{\"pi\":{\"skills\":[\"\xFF\"]}}",
    br#"{"pi":{"skills":["first"],"skills":["second"]}}"#,
];

#[test]
fn pi_manifest_matches_typescript() {
    let directory = tempfile::tempdir().unwrap();
    let mut paths: Vec<String> = MANIFEST_FIXTURES
        .iter()
        .enumerate()
        .map(|(index, contents)| {
            let path = directory.path().join(format!("package-{index}.json"));
            fs::write(&path, contents).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect();
    paths.push(directory.path().join("missing.json").to_string_lossy().into_owned());
    paths.push(directory.path().to_string_lossy().into_owned());

    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/pi-manifest.ts", "piManifest", json!(paths));
    let actual: Vec<Value> = paths
        .iter()
        .map(|path| json!(read_pi_manifest(std::path::Path::new(path))))
        .collect();
    assert_eq!(json!(actual), expected);
}

#[test]
fn experimental_auth_matches_typescript() {
    let options = [
        (None, None),
        (Some("secret"), None),
        (None, Some("/tmp/token")),
        (Some("secret"), Some("/tmp/token")),
        (Some(""), None),
        (None, Some("")),
    ];
    let wire: Vec<Value> = options
        .iter()
        .map(|(token, file)| {
            let mut value = serde_json::Map::new();
            if let Some(token) = token {
                value.insert("authToken".into(), json!(token));
            }
            if let Some(file) = file {
                value.insert("authTokenFile".into(), json!(file));
            }
            Value::Object(value)
        })
        .collect();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("cli/experimental/auth.ts", "auth", json!(wire));
    let actual: Vec<Value> = options
        .iter()
        .map(|(token, file)| {
            let parsed = parse_auth_input(&RawAuthOptions {
                auth_token: token.map(str::to_owned),
                auth_token_file: file.map(str::to_owned),
            });
            let mut value = serde_json::Map::new();
            if let Some(auth) = parsed.auth {
                value.insert("auth".into(), json!(auth));
            }
            value.insert("errors".into(), json!(parsed.errors));
            Value::Object(value)
        })
        .collect();
    assert_eq!(json!(actual), expected);
}

const NOW_ISO: &str = "2026-01-01T00:00:00.000Z";
const NOW_MS: i64 = 1_767_225_600_000;
const STALE_MS: i64 = 300_000;

fn live_health() -> SessionHealthRecord {
    SessionHealthRecord {
        agent_generation: 1,
        pid: Some(1234),
        check_status: SessionCheckStatus::Ok,
        checked_generation: Some(1),
        ..empty_session_health("session-a", NOW_ISO)
    }
}

#[test]
fn session_health_matches_typescript() {
    let goals = [
        Value::String(r#"{"objective":"ship list_sessions"}"#.into()),
        Value::String("raw goal text".into()),
        Value::Null,
        Value::String(String::new()),
        Value::String("   \n ".into()),
        Value::String(r#"  {"objective":"  padded  "}  "#.into()),
        Value::String(r#"{"objective":"   "}"#.into()),
        Value::String(r#"{"objective":5}"#.into()),
        Value::String("[1]".into()),
        Value::String("{bad".into()),
    ];
    let live_cases = json!([
        {"health": {}, "lastActiveAt": "2025-12-31T23:59:00.000Z"},
        {"health": {}, "lastActiveAt": "2025-12-31T23:50:00.000Z"},
        {"health": {}, "lastActiveAt": "2025-12-31T23:55:00.000Z"},
        {"health": {}, "lastActiveAt": null},
        {"health": {}, "lastActiveAt": "not a date"},
        {"health": {"pid": null}, "lastActiveAt": "2026-01-01T00:00:00.000Z"},
        {"health": {"checkStatus": "timeout"}, "lastActiveAt": "2026-01-01T00:00:00.000Z"},
        {"health": {"checkStatus": "dead"}, "lastActiveAt": "2026-01-01T00:00:00.000Z"},
        {"health": {"checkStatus": "never"}, "lastActiveAt": "2026-01-01T00:00:00.000Z"},
        {"health": {}, "lastActiveAt": "2027-01-01T00:00:00.000Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T02:00:00.000+02:00"},
        {"health": {}, "lastActiveAt": "2025-12-31T19:00:00.000-05:00"},
        {"health": {}, "lastActiveAt": "2026-01-01"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00"},
        {"health": {}, "lastActiveAt": "2026-02-30T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T24:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T25:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00-0530"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00.123456Z"},
        {"health": {}, "lastActiveAt": "+002026-01-01T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-02-31T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-32T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-00T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00+23:59"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00+2"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00.5Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00.123456789123Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00,5Z"},
        {"health": {}, "lastActiveAt": "0000-01-01T00:00:00Z"},
        {"health": {}, "lastActiveAt": "-000001-01-01T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T24:00:00.001Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00+00"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00-00:00"},
        {"health": {}, "lastActiveAt": "2026-01-01t00:00:00z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00.Z"},
        {"health": {}, "lastActiveAt": "2026-1-1T00:00:00Z"},
        {"health": {}, "lastActiveAt": " 2026-01-01T00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00Zx"},
        {"health": {}, "lastActiveAt": "2026-01-01 00:00:00Z"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00:00.000+0000"},
        {"health": {}, "lastActiveAt": "2026-01-01T00:00"},
    ]);

    let mut oracle = Oracle::spawn();
    let expected = oracle.call_raw(json!({
        "module": "core/session-health.ts",
        "operation": "sessionHealth",
        "cases": goals,
        "nowIso": NOW_ISO,
        "nowMs": NOW_MS,
        "staleMs": STALE_MS,
        "liveCases": live_cases,
    }));

    let base = empty_session_health("session-a", NOW_ISO);
    let live = live_health();
    let live_status: Vec<Value> = live_cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let mut health = live.clone();
            if case["health"]["pid"].is_null() && case["health"].as_object().unwrap().contains_key("pid") {
                health.pid = None;
            }
            if let Some(status) = case["health"]["checkStatus"].as_str() {
                health.check_status = serde_json::from_value(json!(status)).unwrap();
            }
            let last_active = case["lastActiveAt"].as_str();
            json!(derive_live_status(&health, last_active, NOW_MS, STALE_MS))
        })
        .collect();
    let first = advance_generation_for_pid(&base, 42, NOW_ISO);
    let same = advance_generation_for_pid(&first, 42, NOW_ISO);
    let changed = advance_generation_for_pid(&same, 43, NOW_ISO);
    let zero = advance_generation_for_pid(
        &SessionHealthRecord {
            pid: Some(7),
            ..base.clone()
        },
        7,
        NOW_ISO,
    );
    let actual = json!({
        "empty": base,
        "sticky": [
            is_sticky_dead(&SessionHealthRecord {
                agent_generation: 2,
                check_status: SessionCheckStatus::Dead,
                checked_generation: Some(2),
                ..base.clone()
            }),
            is_sticky_dead(&SessionHealthRecord {
                agent_generation: 3,
                check_status: SessionCheckStatus::Dead,
                checked_generation: Some(2),
                ..base.clone()
            }),
        ],
        "eligible": [
            is_session_eligible_to_receive(&live),
            is_session_eligible_to_receive(&SessionHealthRecord {
                check_status: SessionCheckStatus::Never,
                ..live.clone()
            }),
            is_session_eligible_to_receive(&SessionHealthRecord {
                check_status: SessionCheckStatus::Dead,
                ..live.clone()
            }),
        ],
        "ended": end_session_health(&live, NOW_ISO),
        "generations": [first, same, changed, zero],
        "goals": goals.iter().map(|value| json!(parse_goal_objective(Some(value)))).collect::<Vec<_>>(),
        "liveStatus": live_status,
    });
    assert_eq!(actual, expected);
    assert_eq!(
        serde_json::to_value(SessionLiveStatus::Ended).unwrap(),
        json!("ended"),
        "wire name"
    );
}
