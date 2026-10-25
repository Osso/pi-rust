//! Differential tests: the pinned TypeScript core leaf modules versus the Rust port.

#[path = "support/oracle.rs"]
mod oracle;

use std::fs;

use pi_coding_agent::core::detached_job_retention::{
    DetachedArtifactRetentionCandidate, DetachedArtifactRetentionPolicy, select_detached_artifact_directories_to_delete,
};
use pi_coding_agent::core::http_dispatcher::{
    HttpIdleTimeout, format_http_idle_timeout_ms, parse_http_idle_timeout_ms,
};
use pi_coding_agent::core::permissions::policy::{
    APPROVAL_POLICIES, ApprovalPolicyContext, evaluate_approval_policy, is_approval_policy, normalize_approval_policy,
};
use pi_coding_agent::core::permissions::presets::{
    APPROVAL_PRESETS, SANDBOX_PROFILES, approval_preset_for_policy, approval_preset_to_bypass_permissions,
    find_approval_preset, is_approval_preset_name, is_sandbox_platform_supported, is_sandbox_profile_name,
};
use pi_coding_agent::core::provider_display_names::BUILT_IN_PROVIDER_DISPLAY_NAMES;
use pi_coding_agent::core::runtime_process::command_line_is_pi_runtime;
use pi_coding_agent::core::session_errors::SessionImportFileNotFoundError;
use pi_coding_agent::core::session_health::{
    SessionCheckStatus, SessionHealthRecord, SessionLiveStatus, advance_generation_for_pid, derive_live_status,
    empty_session_health, end_session_health, is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use pi_coding_agent::core::tool_capabilities::{
    CHILD_DISABLED_AGENT_TOOL_NAMES, STANDALONE_DISABLED_TOOL_NAMES, SUPERVISOR_ONLY_TOOL_NAMES,
};
use pi_coding_agent::supervisor::project_resolver::{
    SupervisorProjectConfig, extract_remote_repository_identity, extract_remote_repository_name,
    read_supervisor_project_config, resolve_supervisor_project,
};
use pi_js_compat::js_json::JsString;
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
fn runtime_identity_snapshot_matches_frozen() {
    use pi_coding_agent::core::runtime_process::{
        is_process_identity_alive, read_process_group_id, read_process_identity,
    };
    let pid = std::process::id();
    let identity = read_process_identity(pid).unwrap();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/runtime-process.ts", "runtimeIdentitySnapshot", json!([pid]));
    assert_eq!(
        json!([{
            "identity": identity,
            "group": read_process_group_id(pid).unwrap(),
            "alive": is_process_identity_alive(&identity),
        }]),
        expected
    );
}

#[test]
fn runtime_empty_executable_matches_frozen() {
    let cases = vec![vec!["".to_owned(), "packages/coding-agent/src/cli.ts".to_owned()]];
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/runtime-process.ts", "runtimeProcess", json!(cases));
    let actual = cases
        .iter()
        .map(|argv| command_line_is_pi_runtime(argv))
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), expected);
}

#[test]
fn runtime_identity_missing_process_error_matches_frozen() {
    let mut oracle = Oracle::spawn();
    let pids = [4_194_303_u32];
    let expected = oracle.call("core/runtime-process.ts", "runtimeIdentityError", json!(pids));
    let actual = pids.map(|pid| {
        json!({
            "error": pi_coding_agent::core::runtime_process::read_process_identity(pid).unwrap_err().message,
        })
    });
    assert_eq!(json!(actual), expected);
}

#[test]
fn superseded_runtime_identity_matches_frozen() {
    use pi_coding_agent::core::runtime_process::{ProcessIdentity, is_superseded_runtime_identity};
    let base = ProcessIdentity {
        pid: 123,
        start_time_ticks: 456,
        incarnation: Some("old".into()),
    };
    let replacements = [
        base.clone(),
        ProcessIdentity {
            incarnation: Some("new".into()),
            ..base.clone()
        },
        ProcessIdentity {
            incarnation: Some("".into()),
            ..base.clone()
        },
        ProcessIdentity {
            incarnation: None,
            ..base.clone()
        },
        ProcessIdentity {
            pid: 124,
            incarnation: Some("new".into()),
            ..base.clone()
        },
        ProcessIdentity {
            start_time_ticks: 457,
            incarnation: Some("new".into()),
            ..base.clone()
        },
    ];
    let pairs = replacements
        .iter()
        .flat_map(|replacement| {
            [
                (base.clone(), replacement.clone()),
                (
                    ProcessIdentity {
                        incarnation: None,
                        ..base.clone()
                    },
                    replacement.clone(),
                ),
            ]
        })
        .collect::<Vec<_>>();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/runtime-process.ts", "supersededRuntimeIdentity", json!(pairs));
    let actual = pairs
        .iter()
        .map(|(current, replacement)| is_superseded_runtime_identity(current, replacement))
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), expected);
}

#[test]
fn activity_detail_matches_frozen_for_priorities_whitespace_and_utf16_cut() {
    use pi_coding_agent::core::agent_activity_detail::{MAX_ACTIVITY_DETAIL_LENGTH, summarize_tool_arguments};
    use pi_js_compat::js_json::{JsValue, parse_lossless};
    let mut cases = vec![
        json!(null),
        json!(false),
        json!("command"),
        json!([]),
        json!({}),
        json!({"command": 3, "code": "\t \n", "path": " /tmp/a\n b "}),
        json!({"command": "first", "code": "second", "path": "third"}),
        json!({"command": "a\u{0085}b\u{200b}c", "path": "other"}),
        json!({"command": "x".repeat(160)}),
        json!({"command": "x".repeat(161)}),
        json!({"command": format!("{}😀end", "x".repeat(158))}),
    ];
    for key in [
        "code",
        "path",
        "file_path",
        "pattern",
        "query",
        "url",
        "prompt",
        "message",
        "agentId",
    ] {
        cases.push(json!({key: " chosen\n value ", "unknown": "ignored"}));
    }
    for whitespace in [
        0x9, 0xa, 0xb, 0xc, 0xd, 0x20, 0xa0, 0x1680, 0x2000, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff,
    ] {
        let character = char::from_u32(whitespace).unwrap();
        cases.push(json!({"command": format!("{character}a{character}{character}b{character}")}));
    }
    let mut native = JsValue::from(json!(cases));
    native
        .as_array_mut()
        .unwrap()
        .push(parse_lossless(r#"{"command":"raw-\ud800"}"#).unwrap());
    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let request = JsValue::object([
        ("id", JsValue::from(json!(28001))),
        ("module", JsValue::String("core/agent-activity-detail.ts".into())),
        ("operation", JsValue::String("activityDetail".into())),
        ("cases", native.clone()),
    ]);
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], JsValue::Bool(true));
    let actual = native
        .as_array()
        .unwrap()
        .iter()
        .map(|args| {
            summarize_tool_arguments(args)
                .map(JsValue::String)
                .unwrap_or(JsValue::Null)
        })
        .collect::<Vec<_>>();
    assert_eq!(JsValue::Array(actual), expected["output"]);
    assert_eq!(MAX_ACTIVITY_DETAIL_LENGTH, 160);
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
    let name_cases = NAME_CASES
        .into_iter()
        .chain(["llm-approved-deny", "llm-approved-ask", "never-ask-deny"])
        .collect::<Vec<_>>();
    let expected = oracle.call("core/permissions/presets.ts", "presets", json!(name_cases));
    let supported_platforms = [
        "linux", "darwin", "win32", "freebsd", "aix", "openbsd", "sunos", "android",
    ]
    .map(is_sandbox_platform_supported);
    let actual = json!({
        "approvalPresets": APPROVAL_PRESETS,
        "sandboxProfiles": SANDBOX_PROFILES,
        "supportedPlatforms": supported_platforms,
        "defaultPlatformSupported": is_sandbox_platform_supported(pi_coding_agent::utils::pi_user_agent::node_platform_name(std::env::consts::OS)),
        "found": APPROVAL_PRESETS.iter().map(|preset| find_approval_preset(preset.name)).collect::<Vec<_>>(),
        "forPolicy": APPROVAL_POLICIES.map(approval_preset_for_policy),
        "bypass": APPROVAL_PRESETS.map(|preset| approval_preset_to_bypass_permissions(preset.name)),
        "names": name_cases.into_iter().map(|value| json!({
            "preset": is_approval_preset_name(&value.into()),
            "sandbox": is_sandbox_profile_name(&value.into()),
        })).collect::<Vec<_>>(),
    });
    assert_eq!(actual, expected);
}

#[test]
fn preset_membership_matches_frozen_for_raw_utf16() {
    let cases = vec![
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("read-only") + JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xd800]) + "ask-me",
        JsString::from("auto-approve\0"),
        JsString::from("workspace-write"),
        JsString::from("never-ask-deny"),
    ];
    let mut oracle = Oracle::spawn();
    let expected = oracle.call(
        "core/permissions/presets.ts",
        "presetNameUnits",
        json!(cases.iter().map(|value| value.units()).collect::<Vec<_>>()),
    );
    let actual = cases
        .iter()
        .map(|value| json!({"preset":is_approval_preset_name(value),"sandbox":is_sandbox_profile_name(value)}))
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), expected);
}

#[test]
fn constant_modules_match_typescript() {
    let mut oracle = Oracle::spawn();
    assert_eq!(
        oracle.call("core/provider-display-names.ts", "providerDisplayNames", json!([])),
        json!(BUILT_IN_PROVIDER_DISPLAY_NAMES)
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

#[test]
fn all_tool_capability_exports_and_membership_match_frozen() {
    let queries = [
        "agent_viewer",
        "answer_question",
        "attach_session_agent",
        "close_agent",
        "list_agents",
        "set_agent_model",
        "spawn_agent",
        "steer_agent",
        "wait_agent",
        "ask_supervisor",
        "manage_goal",
        "send_agent_message",
        "contact_parent",
        "list_sessions",
        "broadcast",
        "channel_post",
        "resume_session",
        "read",
        "ask_questions",
        "ask_architect",
        "",
        "__proto__",
        "SPAWN_AGENT",
        "spawn_agent\0",
        "spawn_agent ",
        "不存在",
    ];
    let standalone = STANDALONE_DISABLED_TOOL_NAMES.iter().copied().collect::<Vec<_>>();
    let membership = queries
        .iter()
        .map(|name| json!({"name":name,"disabled":STANDALONE_DISABLED_TOOL_NAMES.contains(*name)}))
        .collect::<Vec<_>>();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/tool-capabilities.ts", "toolCapabilities", json!(queries));
    assert_eq!(
        json!({"child":CHILD_DISABLED_AGENT_TOOL_NAMES,"supervisor":SUPERVISOR_ONLY_TOOL_NAMES,"standalone":standalone,"queries":membership}),
        expected
    );
}

#[test]
fn pi_manifest_discovery_matches_frozen_public_consumer() {
    use pi_coding_agent::core::extensions::loader::discover_extension_paths;

    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().unwrap());
    let agent_dir = JsString::from(directory.path().join("agent").to_str().unwrap());
    let manifests = [
        json!({"pi":{"extensions":["b.ts","a.ts"],"skills":["s"],"prompts":["p"],"themes":["t"]}}),
        json!({"pi":{}}),
        json!({"pi":{"extensions":[]}}),
        json!({"pi":{"extensions":["missing.ts","a.ts","a.ts"]}}),
        json!({"name":"without-pi"}),
    ];
    let paths = manifests
        .iter()
        .enumerate()
        .map(|(index, manifest)| {
            let package = directory.path().join(format!("package-{index}"));
            fs::create_dir(&package).unwrap();
            fs::write(package.join("package.json"), manifest.to_string()).unwrap();
            for name in ["a.ts", "b.ts", "index.ts"] {
                fs::write(package.join(name), "export default function () {}\n").unwrap();
            }
            JsString::from(package.to_str().unwrap())
        })
        .collect::<Vec<_>>();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call_raw(json!({
        "module":"core/extensions/loader.ts", "operation":"manifestDiscovery",
        "cases":paths, "cwd":cwd, "agentDir":agent_dir,
    }));
    let actual = paths
        .iter()
        .map(|path| discover_extension_paths(std::slice::from_ref(path), &cwd, &agent_dir))
        .collect::<Vec<_>>();
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
        "goals": goals
            .iter()
            .map(|value| {
                let goal_json = value.as_str().map(JsString::from);
                json!(parse_goal_objective(goal_json.as_ref()).map(|goal| goal.to_utf8().expect("scalar goal")))
            })
            .collect::<Vec<_>>(),
        "liveStatus": live_status,
    });
    assert_eq!(actual, expected);
    assert_eq!(
        serde_json::to_value(SessionLiveStatus::Ended).unwrap(),
        json!("ended"),
        "wire name"
    );
}

#[test]
fn project_resolver_matches_typescript() {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("projects.json");
    fs::write(
        &config_path,
        r#"{"globalcomix":{"remotes":["globalcomix","globalcomix-api"],"roots":["/syncthing/Sync/Projects/globalcomix"]},"mangahelpers":{"remotes":["mangahelpers","mangahelpers-worker"],"roots":["/syncthing/Sync/Projects/mangahelpers"]},"2":{"roots":["/work"]},"1":{"roots":["/work/one"]},"world-of-osso":{"remotes":["sentinel"]}}"#,
    )
    .unwrap();
    let empty_path = directory.path().join("empty.json");
    fs::write(&empty_path, "{}").unwrap();
    let config_path = config_path.to_string_lossy().into_owned();
    let empty_path = empty_path.to_string_lossy().into_owned();
    let resolve_cases = json!([
        {"configPath": config_path, "cwd": "/syncthing/Sync/Projects/globalcomix/services/api", "remotes": ["unmapped-service"]},
        {"configPath": config_path, "cwd": "/worktrees/manga-worker", "remotes": ["mangahelpers-worker"]},
        {"configPath": config_path, "cwd": "/worktrees/sentinel", "remotes": ["SENTINEL"]},
        {"configPath": config_path, "cwd": "/work/project", "remotes": []},
        {"configPath": config_path, "cwd": "/work/one/child", "remotes": []},
        {"configPath": config_path, "cwd": "/syncthing/Sync/Projects/globalcomix2", "remotes": ["globalcomix2"]},
        {"configPath": empty_path, "cwd": "/repos/pi", "remotes": ["owner/Pi-Mono"]},
        {"configPath": empty_path, "cwd": "/repos/pi/", "remotes": []},
        {"configPath": empty_path, "cwd": "/", "remotes": []},
        {"configPath": empty_path, "cwd": "/repos/pi", "remotes": [""]},
    ]);
    let urls = [
        "git@github.com:earendil-works/pi-mono.git",
        "https://github.com/earendil-works/pi-mono.git",
        "ssh://git@github.com/earendil-works/pi-mono",
        "git@github.com:pi-mono.git",
        "  https://example.com/a/b/c.git  ",
        "https://example.com/a/b/c/",
        "",
        "///",
        "a/b/c/d",
        "https://example.com",
        "HTTPS://Example.com/Owner/Repo",
        "a:b@c:d/e",
        "user@host:/x/y.git",
        "@host:repo",
    ];
    let mut oracle = Oracle::spawn();
    let expected = oracle.call_raw(json!({
        "module": "supervisor/project-resolver.ts",
        "operation": "projectResolver",
        "cases": [],
        "resolveCases": resolve_cases.clone(),
        "urls": urls,
        "configPaths": [config_path.clone(), empty_path.clone(), directory.path().join("missing.json").to_string_lossy()],
    }));

    let config = read_supervisor_project_config(&config_path.as_str().into()).unwrap();
    let to_json = |config: &SupervisorProjectConfig| -> Value {
        let value = pi_js_compat::js_json::to_value(config).unwrap();
        serde_json::from_str(&pi_js_compat::js_json::stringify_lossless(&value)).unwrap()
    };
    let text = |value: Option<JsString>| value.map(|value| value.to_utf8().unwrap());
    let empty = SupervisorProjectConfig::new();
    let resolved: Vec<Value> = resolve_cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let path = case["configPath"].as_str().unwrap();
            let config = if path == config_path { &config } else { &empty };
            let remotes: Vec<JsString> = case["remotes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|remote| remote.as_str().unwrap().into())
                .collect();
            json!(text(Some(
                resolve_supervisor_project(config, &case["cwd"].as_str().unwrap().into(), &remotes).unwrap()
            )))
        })
        .collect();
    let identities: Vec<Value> = urls
        .iter()
        .map(|url| {
            json!({
                "identity": text(extract_remote_repository_identity(&(*url).into())),
                "name": text(extract_remote_repository_name(&(*url).into())),
            })
        })
        .collect();
    let missing: JsString = directory.path().join("missing.json").to_string_lossy().as_ref().into();
    let configs = vec![
        json!({"ok": true, "config": to_json(&config)}),
        json!({"ok": true, "config": to_json(&read_supervisor_project_config(&empty_path.as_str().into()).unwrap())}),
        json!({"ok": true, "config": to_json(&read_supervisor_project_config(&missing).unwrap())}),
    ];
    assert_eq!(
        json!({"resolved": resolved, "identities": identities, "configs": configs}),
        expected
    );
}

#[test]
fn runtime_process_command_line_detection_matches_typescript() {
    let command_lines = [
        vec!["pi"],
        vec!["/usr/local/bin/pi"],
        vec!["/opt/Pi.EXE"],
        vec!["pi-extra"],
        vec!["bash"],
        vec!["node", "/home/osso/Projects/pi/packages/coding-agent/src/cli.ts"],
        vec!["bun", "packages/coding-agent/src/bun/cli.ts"],
        vec!["node", "packages/coding-agent/dist/cli.js"],
        vec!["node", "C:\\work\\packages\\coding-agent\\dist\\cli.js"],
        vec!["node", "dist/cli.js"],
        vec!["node", "packages/coding-agent/src/cli.tsx"],
        vec!["node", "other.js"],
        vec![],
    ];
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/runtime-process.ts", "runtimeProcess", json!(command_lines));
    let actual: Vec<bool> = command_lines
        .iter()
        .map(|command_line| {
            command_line_is_pi_runtime(&command_line.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
        })
        .collect();
    assert_eq!(json!(actual), expected);
}

#[test]
fn http_dispatcher_parsing_and_formatting_match_typescript() {
    let values = json!([
        "disabled",
        "DISABLED",
        " disabled ",
        "",
        "   ",
        "30000",
        " 30000 ",
        "1.9",
        "-1",
        "0",
        "abc",
        "1e3",
        "0x10",
        "0b101",
        "0o17",
        "Infinity",
        "-Infinity",
        "NaN",
        "1.5e",
        ".5",
        "5.",
        30000,
        0,
        1500.9,
        -0.5,
        2.5e-7
    ]);
    let timeouts = json!([0, 30_000, 60_000, 120_000, 300_000, 45_000, 1500, 1234, 999, 1_000_000]);
    let mut oracle = Oracle::spawn();
    let expected = oracle.call_raw(json!({
        "module": "core/http-dispatcher.ts",
        "operation": "httpDispatcher",
        "cases": values,
        "timeouts": timeouts,
    }));

    let parsed: Vec<Value> = values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let parsed = match value {
                Value::String(text) => parse_http_idle_timeout_ms(HttpIdleTimeout::String(text)),
                Value::Number(number) => parse_http_idle_timeout_ms(HttpIdleTimeout::Number(number.as_f64().unwrap())),
                _ => None,
            };
            json!(parsed)
        })
        .collect();
    let formatted: Vec<String> = timeouts
        .as_array()
        .unwrap()
        .iter()
        .map(|value| format_http_idle_timeout_ms(value.as_f64().unwrap()))
        .collect();
    assert_eq!(json!({"parsed": parsed, "formatted": formatted}), expected);

    // Typed-representation boundary: JavaScript numbers above u64::MAX are not
    // representable as a Rust millisecond duration, so they saturate.
    assert_eq!(
        parse_http_idle_timeout_ms(HttpIdleTimeout::Number(1e21)),
        Some(u64::MAX)
    );
}

#[test]
fn detached_job_retention_matches_typescript() {
    let candidate = |path: &str, size: f64, terminal_at: f64, protected: bool| json!({"directoryPath": path, "byteSize": size, "terminalAt": terminal_at, "protectedByLiveReference": protected});
    let cases = json!([
        {
            "candidates": [
                candidate("/jobs/recent", 10.0, 9_001.0, false),
                candidate("/jobs/boundary", 20.0, 9_000.0, false),
                candidate("/jobs/old", 30.0, 8_000.0, false),
                candidate("/jobs/protected-old", 40.0, 8_000.0, true),
            ],
            "policy": {"now": 10_000.0, "maxAge": 1_000.0, "maxBytes": 1_000.0},
        },
        {
            "candidates": [
                candidate("/jobs/newest", 30.0, 9_900.0, false),
                candidate("/jobs/oldest", 30.0, 9_700.0, false),
                candidate("/jobs/middle", 30.0, 9_800.0, false),
                candidate("/jobs/protected", 50.0, 9_600.0, true),
            ],
            "policy": {"now": 10_000.0, "maxAge": 10_000.0, "maxBytes": 90.0},
        },
        {
            "candidates": [candidate("/jobs/z", 10.0, 9_500.0, false), candidate("/jobs/a", 10.0, 9_500.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 10_000.0, "maxBytes": 10.0},
        },
        {
            "candidates": [candidate("/jobs/A", 10.0, 9_500.0, false), candidate("/jobs/a", 10.0, 9_500.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 10_000.0, "maxBytes": 10.0},
        },
        {
            "candidates": [candidate("/jobs/B", 10.0, 9_500.0, false), candidate("/jobs/z", 10.0, 9_500.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 10_000.0, "maxBytes": 10.0},
        },
        {
            "candidates": [candidate("/jobs/protected", 200.0, 0.0, true), candidate("/jobs/keep", 10.0, 9_900.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 1_000.0, "maxBytes": 100.0},
        },
        {"candidates": [], "policy": {"now": 10_000.0, "maxAge": 1.0, "maxBytes": 1.0}},
        {
            "candidates": [candidate("/jobs/a", 10.0, 9_999.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 10_000.0, "maxBytes": 100.0},
        },
        {
            "candidates": [candidate("/jobs/a", -1.0, 0.0, false)],
            "policy": {"now": 10_000.0, "maxAge": 1.0, "maxBytes": 1.0},
        },
        {"candidates": [], "policy": {"now": -1.0, "maxAge": 1.0, "maxBytes": 1.0}},
        {"candidates": [], "policy": {"now": 10_000.0, "maxAge": 1.0, "maxBytes": -1.0}},
    ]);
    let mut oracle = Oracle::spawn();
    let expected = oracle.call("core/detached-job-retention.ts", "detachedJobRetention", cases.clone());

    let actual: Vec<Value> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let candidates: Vec<DetachedArtifactRetentionCandidate> = case["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|candidate| DetachedArtifactRetentionCandidate {
                    directory_path: JsString::from(candidate["directoryPath"].as_str().unwrap().to_owned()),
                    byte_size: candidate["byteSize"].as_f64().unwrap(),
                    terminal_at: candidate["terminalAt"].as_f64().unwrap(),
                    protected_by_live_reference: candidate["protectedByLiveReference"].as_bool().unwrap(),
                })
                .collect();
            let policy = DetachedArtifactRetentionPolicy {
                now: case["policy"]["now"].as_f64().unwrap(),
                max_age: case["policy"]["maxAge"].as_f64().unwrap(),
                max_bytes: case["policy"]["maxBytes"].as_f64().unwrap(),
            };
            match select_detached_artifact_directories_to_delete(&candidates, &policy) {
                Ok(paths) => json!({"ok": true, "paths": paths}),
                Err(error) => json!({"ok": false, "error": error}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected);
}
