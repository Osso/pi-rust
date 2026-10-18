//! Differential tests: the pinned TypeScript core leaf modules versus the Rust port.

#[path = "support/oracle.rs"]
mod oracle;

use std::fs;

use pi_coding_agent::cli::experimental::auth::{RawAuthOptions, parse_auth_input};
use pi_coding_agent::cli::experimental::command::{
    Command, CommandOption, CommandParseResult, NamedCommandInvocation, string_option, value_option,
};
use pi_coding_agent::cli::experimental::transport_address::parse_transport_address;
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
use pi_coding_agent::core::runtime_process::command_line_is_pi_runtime;
use pi_coding_agent::core::session_errors::SessionImportFileNotFoundError;
use pi_coding_agent::core::session_health::{
    SessionCheckStatus, SessionHealthRecord, SessionLiveStatus, advance_generation_for_pid, derive_live_status,
    empty_session_health, end_session_health, is_session_eligible_to_receive, is_sticky_dead, parse_goal_objective,
};
use pi_coding_agent::core::tool_capabilities::{CHILD_DISABLED_AGENT_TOOL_NAMES, SUPERVISOR_ONLY_TOOL_NAMES};
use pi_coding_agent::supervisor::project_resolver::{
    SupervisorProjectConfig, extract_remote_repository_identity, extract_remote_repository_name,
    read_supervisor_project_config, resolve_supervisor_project,
};
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

#[test]
fn transport_address_matches_typescript() {
    let values = [
        "unix:///tmp/pi.sock",
        "unix:///",
        "unix:///tmp/pi.sock/",
        "unix:///tmp/a%20b",
        "unix:///tmp/a%2Fb",
        "unix:///tmp/%C3%BC",
        "unix:///tmp/%zz",
        "unix:///tmp/%2",
        "unix:///tmp/%00pi.sock",
        "unix:///tmp/%2E%2E/pi.sock",
        "unix:///tmp/../etc/passwd",
        "unix:////tmp/pi.sock",
        "unix:/tmp/pi.sock",
        "/tmp/pi.sock",
        "",
        "unix://relative.sock",
        "unix://user@host/tmp/x",
        "unix://host:1234/tmp/x",
        "unix:///tmp/pi.sock?x=1",
        "unix:///tmp/pi.sock#frag",
        "unix:///tmp/pi sock",
        "UNIX:///tmp/pi.sock",
        "ws://localhost:8080",
        "http://localhost/pi.sock",
        "file:///tmp/pi.sock",
    ];
    let cases: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let option = if index % 2 == 0 { "--listen" } else { "--connect" };
            json!({"value": value, "option": option})
        })
        .collect();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call(
        "cli/experimental/transport-address.ts",
        "transportAddress",
        json!(cases),
    );
    let actual: Vec<Value> = cases
        .iter()
        .map(|case| {
            let value = case["value"].as_str().unwrap();
            let option = case["option"].as_str().unwrap();
            match parse_transport_address(value, option) {
                Ok(address) => json!({"ok": true, "address": address}),
                Err(error) => json!({"ok": false, "error": error}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliInvocation {
    command: String,
    name: Option<String>,
    count: Option<i64>,
    remaining: Vec<String>,
}

impl NamedCommandInvocation for CliInvocation {
    fn command(&self) -> &str {
        &self.command
    }
}

fn cli_invocation(
    command: &str,
    input: &pi_coding_agent::cli::experimental::command::ParsedCommandInput<'_>,
    name: &CommandOption<String>,
    count: &CommandOption<i64>,
) -> CliInvocation {
    CliInvocation {
        command: command.to_owned(),
        name: input.value(name),
        count: input.value(count),
        remaining: input.remaining_args.to_vec(),
    }
}

fn build_cli_command() -> Command<CliInvocation, Vec<String>> {
    let name = string_option("--name");
    let count = value_option("--count", |value| {
        value
            .parse::<i64>()
            .map_err(|_| format!("--count must be a number, got \"{value}\""))
    });
    let mut root: Command<CliInvocation, Vec<String>> = Command::new("pi");
    root.option(name.clone());
    root.option(count.clone());
    root.build(move |input| CommandParseResult::Ok(cli_invocation("pi", input, &name, &count)));
    root.action(|_, context| context.push("pi".into()));

    let listen = string_option("--listen");
    let mut child: Command<CliInvocation, Vec<String>> = Command::new("server");
    child.option(listen.clone());
    child.build(move |input| {
        CommandParseResult::Ok(CliInvocation {
            command: "server".into(),
            name: input.value(&listen),
            count: None,
            remaining: input.remaining_args.to_vec(),
        })
    });
    child.action(|_, context| context.push("server".into()));
    root.command(child, |invocation| invocation);
    root
}

const CLI_ARGV: [&[&str]; 18] = [
    &[],
    &["--name", "demo"],
    &["--name=demo"],
    &["--name", "demo", "--count", "3", "inspect", "the project"],
    &["--name"],
    &["--name="],
    &["--name", "--count", "2"],
    &["--name", "a", "--name=b"],
    &["--count", "many", "--name", "demo"],
    &["--count", "-3"],
    &["--", "--name", "demo"],
    &["--unknown", "--name", "demo"],
    &["server", "--listen", "unix:///tmp/pi.sock"],
    &["server", "--name", "demo"],
    &["--name", "server"],
    &["server"],
    &["server", "--listen"],
    &["-x", "--name", "demo"],
];

#[test]
fn command_parsing_matches_typescript() {
    let argv_cases: Vec<Value> = CLI_ARGV
        .iter()
        .map(|argv| json!(argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>()))
        .collect();
    let mut oracle = Oracle::spawn();
    let expected = oracle.call_raw(json!({
        "module": "cli/experimental/command.ts",
        "operation": "command",
        "cases": [],
        "argvCases": argv_cases,
    }));
    let command = build_cli_command();
    let actual: Vec<Value> = CLI_ARGV
        .iter()
        .map(|argv| {
            let argv: Vec<String> = argv.iter().map(|value| (*value).to_owned()).collect();
            match command.parse(&argv) {
                CommandParseResult::Ok(invocation) => json!({
                    "ok": true,
                    "command": {
                        "command": invocation.command,
                        "name": invocation.name,
                        "count": invocation.count,
                        "remaining": invocation.remaining,
                    }
                }),
                CommandParseResult::Errors(errors) => json!({"ok": false, "errors": errors}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected);
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

    let config = read_supervisor_project_config(std::path::Path::new(&config_path)).unwrap();
    let empty = SupervisorProjectConfig::new();
    let resolved: Vec<Value> = resolve_cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let path = case["configPath"].as_str().unwrap();
            let config = if path == config_path { &config } else { &empty };
            let remotes: Vec<String> = case["remotes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|remote| remote.as_str().unwrap().to_owned())
                .collect();
            json!(resolve_supervisor_project(
                config,
                case["cwd"].as_str().unwrap(),
                &remotes
            ))
        })
        .collect();
    let identities: Vec<Value> = urls
        .iter()
        .map(|url| {
            json!({
                "identity": extract_remote_repository_identity(url),
                "name": extract_remote_repository_name(url),
            })
        })
        .collect();
    let missing = directory.path().join("missing.json");
    let configs = vec![
        json!({"ok": true, "config": read_supervisor_project_config(std::path::Path::new(&config_path)).unwrap()}),
        json!({"ok": true, "config": read_supervisor_project_config(std::path::Path::new(&empty_path)).unwrap()}),
        json!({"ok": true, "config": read_supervisor_project_config(&missing).unwrap()}),
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
