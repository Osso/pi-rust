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
        self.next_id += 1;
        let response = self.inner.request(&json!({
            "id": self.next_id,
            "module": module,
            "operation": operation,
            "cases": cases,
        }));
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
