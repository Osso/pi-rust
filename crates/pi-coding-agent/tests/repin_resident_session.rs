//! Actual-source filesystem/metadata interoperability, not a runtime impersonation.
//! Frozen-baseline-with-approved-delta 8f8f3c26: frozen matrices assert exactly
//! the 42 approved metadata differences; delta matrices assert none. Both run in
//! the ordinary gate through the mandatory guard and tracked delta lock.
#![cfg(unix)]
#[path = "support/resident_frozen_delta.rs"]
mod frozen_delta;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use pi_coding_agent::core::resident_session::{ResidentRole, ResidentSessionError, prepare_resident_session_file};
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions, WritableSessionMetadata, list_session_metadata,
    register_runtime_mailbox_listener, write_session_metadata,
};
use serde_json::{Value, json};

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn_with_environment(
        "resident-session-oracle.ts",
        "packages/coding-agent/src",
        &[("PI_RESIDENT_SESSION_DELTA_MODULE", "".into())],
    )
}

fn request(oracle: &mut oracle::TsOracle, op: &str, fixture: &Value) -> Value {
    oracle.request(&json!({"op": op, "fixture": fixture}))
}

fn success(oracle: &mut oracle::TsOracle, op: &str, fixture: &Value) -> Value {
    let result = request(oracle, op, fixture);
    assert_eq!(result["ok"], true, "{op}: {result}");
    result["value"].clone()
}

fn fixture(root: &str, role: &str, case: &str) -> Value {
    let dir = format!("{root}/agent/{role}-sessions");
    let other = if role == "architect" { "supervisor" } else { "architect" };
    let mut f = json!({"root": root, "agentDir": format!("{root}/agent"), "role": role,
        "db": format!("{root}/control.sqlite"), "entries": [], "metadata": []});
    let canonical = format!("{dir}/{role}.jsonl");
    let older = format!("{dir}/100_{role}.jsonl");
    let latest = format!("{dir}/900_{role}.jsonl");
    if case != "empty" {
        f["entries"] = json!([
            {"path": canonical}, {"path": older}, {"path": latest},
            {"path": format!("{dir}/unrelated.jsonl")},
            {"path": format!("{dir}/a_{role}.jsonl.bak")},
            {"path": format!("{dir}/z_{role}.jsonl"), "kind": "dir"},
            {"path": format!("{dir}/zz_{role}.jsonl"), "kind": "symlink", "target": "unrelated.jsonl"},
            {"path": format!("{dir}/zzz_{role}.jsonl"), "kind": "symlink", "target": "missing"}
        ]);
        f["metadata"] = json!([
            {"sessionPath": canonical, "id": "canonical"}, {"sessionPath": older, "id": "older"},
            {"sessionPath": latest, "id": "latest"},
            {"sessionPath": format!("{root}/missing-role-id.jsonl"), "id": role},
            {"sessionPath": format!("{root}/{role}-sessions/missing.jsonl"), "id": "segment"},
            {"sessionPath": format!("{root}\\{role}-sessions\\missing.jsonl"), "id": "backslash"},
            {"sessionPath": format!("{root}/{other}-sessions/missing.jsonl"), "id": "other-role"},
            {"sessionPath": format!("{root}/x{role}-sessions/missing.jsonl"), "id": "prefix"},
            {"sessionPath": format!("{root}/{role}-sessions-x/missing.jsonl"), "id": "suffix"},
            {"sessionPath": format!("{root}/ordinary-missing.jsonl"), "id": "ordinary"},
            {"sessionPath": format!("{dir}/zzz_{role}.jsonl"), "id": "broken-link"},
            {"sessionPath": format!("{dir}/zz_{role}.jsonl"), "id": "existing-link"}
        ]);
    }
    match case {
        "canonical-only" => f["entries"] = json!([{"path": canonical}]),
        "no-matching" => {
            f["entries"].as_array_mut().unwrap().drain(0..3);
        }
        "underscore-prefix" => f["entries"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path": format!("{dir}/_{role}.jsonl")})),
        "absolute-dots" => f["agentDir"] = json!(format!("{root}/absent/../agent/./")),
        "unicode" => {
            f["entries"].as_array_mut().unwrap().extend([
                json!({"path": format!("{dir}/\u{10000}_{role}.jsonl")}),
                json!({"path": format!("{dir}/\u{e000}_{role}.jsonl")}),
            ]);
        }
        "relative" => {
            let cwd = std::env::current_dir().unwrap();
            let relative = encode_node_utf8(
                &pi_coding_agent::utils::paths::relative_path(cwd.display().to_string(), root).expect("relative path"),
            );
            f["agentDir"] = json!(format!("{relative}/absent/../agent/./"));
        }
        "self-existing"
        | "self-missing"
        | "self-unmatched"
        | "self-directory"
        | "self-relative"
        | "self-outside"
        | "self-extension"
        | "self-symlink-dir"
        | "self-agent"
        | "no-path"
        | "self-empty-path"
        | "self-trailing-slash"
        | "self-live-symlink"
        | "dead"
        | "nonpi" => {
            let path = match case {
                "self-existing" | "self-agent" | "dead" | "nonpi" => older,
                "self-missing" => format!("{dir}/missing_{role}.jsonl"),
                "self-unmatched" => format!("{dir}/unmatched.jsonl"),
                "self-directory" => format!("{dir}/z_{role}.jsonl"),
                "self-trailing-slash" => format!("{older}/"),
                "self-live-symlink" => format!("{dir}/zz_{role}.jsonl"),
                "self-relative" => {
                    let cwd = std::env::current_dir().unwrap();
                    encode_node_utf8(
                        &pi_coding_agent::utils::paths::relative_path(
                            cwd.display().to_string(),
                            format!("{dir}/./sub/../100_{role}.jsonl"),
                        )
                        .expect("relative path"),
                    )
                }
                "self-outside" => format!("{dir}/child/nested.jsonl"),
                "self-extension" => format!("{dir}/100_{role}.JSONL"),
                "self-symlink-dir" => {
                    f["entries"].as_array_mut().unwrap().push(json!({"path": format!("{root}/alias"), "kind": "symlink", "target": format!("{root}/agent/{role}-sessions")}));
                    format!("{root}/alias/100_{role}.jsonl")
                }
                _ => String::new(),
            };
            f["listener"] = json!({"path": path, "process": if case == "dead" { "dead" } else if case == "nonpi" { "nonpi" } else { "self" }});
            if case == "self-agent" {
                f["listener"]["agentId"] = json!("child");
            }
            if case == "no-path" {
                f["listener"].as_object_mut().unwrap().remove("path");
            }
            if case == "self-missing" {
                f["metadata"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"sessionPath": path, "id": role}));
            }
        }
        "unlink-error" => f["mode"] = json!(0o555),
        "mkdir-error" => {
            f["entries"] = json!([{"path": dir}]);
            f["metadata"] = json!([]);
        }
        "ancestor-error" => {
            f["agentDir"] = json!(format!("{root}/block/child"));
            f["entries"] = json!([{"path": format!("{root}/block")}]);
            f["metadata"] = json!([]);
        }
        "scandir-error" => f["mode"] = json!(0o000),
        "parent-permission-error" => {
            f["agentDir"] = json!(format!("{root}/blocked/missing"));
            f["entries"] = json!([{"path": format!("{root}/blocked"), "kind": "dir"}]);
            f["metadata"] = json!([]);
            f["permissions"] = json!([{"path": format!("{root}/blocked"), "mode": 0o555}]);
        }
        "symlink-loop-error" => {
            f["agentDir"] = json!(format!("{root}/loop"));
            f["entries"] = json!([{"path": format!("{root}/loop"), "kind": "symlink", "target": "loop"}]);
            f["metadata"] = json!([]);
        }
        "symlink-file-error" => {
            f["entries"] = json!([{"path": format!("{root}/plain")}, {"path": dir, "kind": "symlink", "target": format!("{root}/plain")}]);
            f["metadata"] = json!([]);
        }
        _ => {}
    }
    f
}

fn seed_native(f: &Value) {
    for entry in f["entries"].as_array().unwrap() {
        let path = std::path::Path::new(entry["path"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        match entry["kind"].as_str() {
            Some("dir") => fs::create_dir_all(path).unwrap(),
            Some("symlink") => symlink(entry["target"].as_str().unwrap(), path).unwrap(),
            _ => fs::write(path, "fixture\n").unwrap(),
        }
    }
    for row in f["metadata"].as_array().unwrap() {
        let metadata: WritableSessionMetadata = serde_json::from_value(json!({
            "sessionPath": row["sessionPath"], "id": row["id"], "cwd": "/fixture",
            "createdAt": "2026-10-21T00:00:00.000Z", "modifiedAt": "2026-10-21T00:00:00.000Z",
            "messageCount": 0, "firstMessage": ""
        }))
        .unwrap();
        write_session_metadata(f["db"].as_str().unwrap(), &metadata).unwrap();
    }
    if let Some(listener) = f.get("listener") {
        assert_eq!(listener["process"], "self", "dead/nonpi fixtures are source-created");
        register_runtime_mailbox_listener(
            f["db"].as_str().unwrap(),
            &RuntimeMailboxAddress {
                session_id: f["role"].as_str().unwrap().into(),
                agent_id: listener["agentId"].as_str().map(str::to_owned).map(JsString::from),
            },
            std::process::id(),
            listener["path"].as_str().map(JsString::from).as_ref(),
            RuntimeMailboxRegistrationOptions {
                reconcile_runtime_replacement: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    }
    if let Some(mode) = f["mode"].as_u64() {
        fs::set_permissions(
            format!(
                "{}/agent/{}-sessions",
                f["root"].as_str().unwrap(),
                f["role"].as_str().unwrap()
            ),
            fs::Permissions::from_mode(mode as u32),
        )
        .unwrap();
    }
}

fn prepare_native(f: &Value) -> Value {
    let role = match f["role"].as_str().unwrap() {
        "architect" => ResidentRole::Architect,
        _ => ResidentRole::Supervisor,
    };
    match prepare_resident_session_file(
        &JsString::from(f["agentDir"].as_str().unwrap()),
        role,
        &JsString::from(f["db"].as_str().unwrap()),
    ) {
        Ok(result) => json!({"ok": true, "value": result}),
        Err(ResidentSessionError::PathArgument(error)) => json!({"ok": false, "error": {
            "name": error.name, "message": error.message.to_utf8_lossy(), "code": error.code
        }, "messageUnits": error.message.units()}),
        Err(ResidentSessionError::Filesystem(error)) => json!({"ok": false, "error": {
            "name": error.name, "message": error.message, "code": error.code,
            "errno": error.errno, "syscall": error.syscall, "path": error.path
        }}),
        Err(error) => json!({"ok": false, "error": {"name": "Error", "message": error.to_string()}}),
    }
}

fn normalized(value: Value, root: &str) -> Value {
    serde_json::from_str(&serde_json::to_string(&value).unwrap().replace(root, "ROOT")).unwrap()
}

fn restore_permissions(f: &Value) {
    if let Some(entries) = f["permissions"].as_array() {
        for entry in entries {
            fs::set_permissions(entry["path"].as_str().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    if f.get("mode").is_some() {
        fs::set_permissions(
            format!(
                "{}/agent/{}-sessions",
                f["root"].as_str().unwrap(),
                f["role"].as_str().unwrap()
            ),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
}

#[test]
fn frozen_approved_delta_8f8f3c26_reciprocal_snapshot_differences_are_exact() {
    run_source_created_trees_native_prepare_and_source_reader_reciprocal(
        oracle(),
        frozen_delta::expected_discrepancies("reciprocal"),
    );
}

fn run_source_created_trees_native_prepare_and_source_reader_reciprocal(
    mut oracle: oracle::TsOracle,
    expected: Vec<Value>,
) {
    let mut mismatches = Vec::new();
    for role in ["architect", "supervisor"] {
        for case in [
            "empty",
            "ordinary",
            "unicode",
            "relative",
            "canonical-only",
            "no-matching",
            "underscore-prefix",
            "absolute-dots",
        ] {
            let mut outputs = Vec::new();
            for native in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let root = root.path().display().to_string();
                let f = fixture(&root, role, case);
                if native {
                    success(&mut oracle, "seed", &f);
                } else {
                    seed_native(&f);
                }
                let prepared = if native {
                    prepare_native(&f)
                } else {
                    request(&mut oracle, "prepare", &f)
                };
                assert_eq!(prepared["ok"], true, "{role}/{case}: {prepared}");
                let observed = success(&mut oracle, "inspect", &f);
                assert_eq!(
                    list_session_metadata(f["db"].as_str().unwrap()).unwrap().len(),
                    observed["metadata"].as_array().unwrap().len()
                );
                outputs.push(normalized(json!({"prepared": prepared, "observed": observed}), &root));
            }
            if let Some(difference) =
                frozen_delta::snapshot_difference(&format!("{role}/{case}"), &outputs[0], &outputs[1]).unwrap()
            {
                mismatches.push(difference);
            }
            let path = outputs[0]["prepared"]["value"]["sessionPath"].as_str().unwrap();
            let expected = match case {
                "empty" | "canonical-only" | "no-matching" => format!("{role}.jsonl"),
                "underscore-prefix" => format!("_{role}.jsonl"),
                "unicode" => format!("\u{e000}_{role}.jsonl"),
                _ => format!("900_{role}.jsonl"),
            };
            assert!(path.ends_with(&expected), "{case}: {path}");
        }
    }
    frozen_delta::verify_discrepancies(&mismatches, &expected).unwrap();
}

#[test]
fn frozen_approved_delta_8f8f3c26_own_pid_snapshot_differences_are_exact() {
    run_paired_actual_self_pids_cover_live_validation_without_impersonating_pi(
        oracle(),
        frozen_delta::expected_discrepancies("ownPid"),
    );
}

fn run_paired_actual_self_pids_cover_live_validation_without_impersonating_pi(
    mut oracle: oracle::TsOracle,
    expected: Vec<Value>,
) {
    let mut mismatches = Vec::new();
    for role in ["architect", "supervisor"] {
        for case in [
            "self-existing",
            "self-missing",
            "self-unmatched",
            "self-directory",
            "self-relative",
            "self-outside",
            "self-extension",
            "self-symlink-dir",
            "self-agent",
            "no-path",
            "self-empty-path",
            "self-trailing-slash",
            "self-live-symlink",
        ] {
            let mut outputs = Vec::new();
            for native in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let root = temp.path().display().to_string();
                let f = fixture(&root, role, case);
                if native {
                    seed_native(&f);
                } else {
                    success(&mut oracle, "seed", &f);
                }
                let prepared = if native {
                    prepare_native(&f)
                } else {
                    request(&mut oracle, "prepare", &f)
                };
                assert_eq!(prepared["ok"], true, "{case}: {prepared}");
                let observed = success(&mut oracle, "inspect", &f);
                outputs.push(normalized(json!({"prepared": prepared, "observed": observed}), &root));
            }
            if let Some(difference) =
                frozen_delta::snapshot_difference(&format!("{role}/{case}"), &outputs[0], &outputs[1]).unwrap()
            {
                mismatches.push(difference);
            }
        }
    }
    frozen_delta::verify_discrepancies(&mismatches, &expected).unwrap();
}

#[test]
fn frozen_approved_delta_8f8f3c26_owned_child_snapshot_differences_are_exact() {
    run_source_owned_dead_and_live_non_pi_children_are_not_resident_runtimes(
        oracle(),
        frozen_delta::expected_discrepancies("ownedChild"),
    );
}

fn run_source_owned_dead_and_live_non_pi_children_are_not_resident_runtimes(
    mut oracle: oracle::TsOracle,
    expected: Vec<Value>,
) {
    let mut mismatches = Vec::new();
    for case in ["dead", "nonpi"] {
        let mut outputs = Vec::new();
        for native in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().display().to_string();
            let f = fixture(&root, "supervisor", case);
            success(&mut oracle, "seed", &f);
            let prepared = if native {
                prepare_native(&f)
            } else {
                request(&mut oracle, "prepare", &f)
            };
            assert_eq!(prepared["ok"], true, "{case}: {prepared}");
            outputs.push(normalized(
                json!({"prepared": prepared, "observed": success(&mut oracle, "inspect", &f)}),
                &root,
            ));
        }
        if let Some(difference) = frozen_delta::snapshot_difference(case, &outputs[0], &outputs[1]).unwrap() {
            mismatches.push(difference);
        }
        assert!(
            outputs[0]["prepared"]["value"]["sessionPath"]
                .as_str()
                .unwrap()
                .ends_with("900_supervisor.jsonl")
        );
    }
    frozen_delta::verify_discrepancies(&mismatches, &expected).unwrap();
}

#[test]
fn actual_source_filesystem_errors_and_metadata_before_unlink_order() {
    let mut oracle = oracle();
    for case in [
        "mkdir-error",
        "ancestor-error",
        "scandir-error",
        "unlink-error",
        "parent-permission-error",
        "symlink-loop-error",
        "symlink-file-error",
    ] {
        let mut messages = Vec::new();
        for native in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().display().to_string();
            let f = fixture(&root, "architect", case);
            success(&mut oracle, "seed", &f);
            let outcome = if native {
                prepare_native(&f)
            } else {
                request(&mut oracle, "prepare", &f)
            };
            restore_permissions(&f);
            assert_eq!(outcome["ok"], false, "{case}: {outcome}");
            messages.push(normalized(outcome["error"].clone(), &root));
            if case == "unlink-error" {
                let observed = success(&mut oracle, "inspect", &f);
                let paths: Vec<_> = observed["metadata"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["sessionPath"].as_str().unwrap())
                    .collect();
                assert!(
                    !paths.iter().any(|path| path.ends_with("/100_architect.jsonl")),
                    "stale metadata removed before failed unlink"
                );
                assert!(
                    paths.iter().any(|path| path.ends_with("/architect.jsonl")),
                    "later stale row not reached"
                );
                assert!(fs::exists(format!("{root}/agent/architect-sessions/100_architect.jsonl")).unwrap());
            }
        }
        assert_eq!(messages[0], messages[1], "source/native {case}");
    }
}

#[test]
fn invalid_control_database_propagates_after_mkdir_before_file_deletion() {
    let mut oracle = oracle();
    for native in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().display().to_string();
        let mut f = fixture(&root, "supervisor", "empty");
        fs::write(format!("{root}/not-db"), "not sqlite").unwrap();
        f["db"] = json!(format!("{root}/not-db"));
        let result = if native {
            prepare_native(&f)
        } else {
            request(&mut oracle, "prepare", &f)
        };
        assert_eq!(result["ok"], false, "database error must propagate: {result}");
        assert!(
            fs::metadata(format!("{root}/agent/supervisor-sessions"))
                .unwrap()
                .is_dir()
        );
    }
}

// File/path/liveness proof is independent of approved frozen metadata differences.
// All metadata rows, including backslash cases, remain in these trees.
#[test]
fn independent_selection_unicode_relative_types_and_paired_liveness() {
    run_independent_selection_unicode_relative_types_and_paired_liveness(oracle());
}

fn run_independent_selection_unicode_relative_types_and_paired_liveness(mut oracle: oracle::TsOracle) {
    for role in ["architect", "supervisor"] {
        for case in [
            "empty",
            "ordinary",
            "unicode",
            "relative",
            "canonical-only",
            "no-matching",
            "underscore-prefix",
            "absolute-dots",
            "self-existing",
            "self-missing",
            "self-unmatched",
            "self-directory",
            "self-relative",
            "self-outside",
            "self-extension",
            "self-symlink-dir",
            "self-agent",
            "no-path",
            "self-empty-path",
            "self-trailing-slash",
            "self-live-symlink",
            "dead",
            "nonpi",
        ] {
            let mut outputs = Vec::new();
            for native in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let root = temp.path().display().to_string();
                let f = fixture(&root, role, case);
                // Source/native own-PID cases are paired, never interchanged.
                // Other source-created trees feed native; native trees feed TS.
                if (native && !case.starts_with("self-") && case != "no-path")
                    || case == "dead"
                    || case == "nonpi"
                    || (!native && (case.starts_with("self-") || case == "no-path"))
                {
                    success(&mut oracle, "seed", &f);
                } else {
                    seed_native(&f);
                }
                let prepared = if native {
                    prepare_native(&f)
                } else {
                    request(&mut oracle, "prepare", &f)
                };
                assert_eq!(prepared["ok"], true, "{role}/{case}: {prepared}");
                let observed = success(&mut oracle, "inspect", &f);
                if case == "self-missing" {
                    assert!(
                        observed["metadata"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|row| row["sessionPath"] == f["listener"]["path"]),
                        "missing live path metadata must survive: {observed}"
                    );
                }
                let entries = observed["entries"].clone();
                outputs.push(normalized(json!({"prepared": prepared, "entries": entries}), &root));
            }
            assert_eq!(outputs[0], outputs[1], "{role}/{case}: concrete paths/files/types");
            let expected = match case {
                "empty" | "canonical-only" | "no-matching" => format!("{role}.jsonl"),
                "underscore-prefix" => format!("_{role}.jsonl"),
                "unicode" => format!("\u{e000}_{role}.jsonl"),
                "self-existing" | "self-relative" | "self-trailing-slash" => format!("100_{role}.jsonl"),
                "self-live-symlink" => format!("zz_{role}.jsonl"),
                "self-missing" => format!("missing_{role}.jsonl"),
                "self-unmatched" => "unmatched.jsonl".to_owned(),
                "self-directory" => format!("z_{role}.jsonl"),
                _ => format!("900_{role}.jsonl"),
            };
            assert!(
                outputs[0]["prepared"]["value"]["sessionPath"]
                    .as_str()
                    .unwrap()
                    .ends_with(&expected),
                "{case}: {}",
                outputs[0]
            );
            if case == "empty" {
                assert_eq!(
                    outputs[0]["entries"],
                    json!([]),
                    "canonical path selected without creating contents"
                );
            }
        }
    }
}

#[test]
fn nul_path_shared_argument_validation_boundary_is_explicit() {
    let mut oracle = oracle();
    let cases = [
        "/tmp/nul/agent\0tail".to_owned(),
        "/tmp/nul/quote'\0tail".to_owned(),
        "/tmp/nul/both'\"\0tail".to_owned(),
        "/tmp/nul/all'\"`\0tail".to_owned(),
        "/tmp/nul/${quote}'\"`\0tail".to_owned(),
        "/tmp/nul/control\u{1}\u{8}\u{b}\u{c}\r\t\u{1b}\u{7f}\u{85}\0tail".to_owned(),
        format!("/tmp/nul/{}\0tail", "a".repeat(300)),
        format!("/tmp/nul/{}\0tail", "b".repeat(10100)),
        format!("/tmp/nul/{}\n{}\0tail", "x".repeat(100), "y".repeat(100)),
        format!("/tmp/nul/{}\0{}'tail", "x".repeat(10000), "y".repeat(100)),
        format!("/tmp/{}😀\0tail", "a".repeat(121)),
    ];
    for path in cases {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().display().to_string();
        let mut f = fixture(&root, "architect", "empty");
        let prefix = format!("{root}/fresh/");
        f["agentDir"] = if path.contains('😀') {
            // Include only the high surrogate at the inspected 128-unit cutoff.
            json!(format!(
                "{prefix}{}😀\0tail",
                "a".repeat(126 - prefix.encode_utf16().count())
            ))
        } else {
            json!(format!("{prefix}{}", path.strip_prefix("/tmp/").unwrap()))
        };
        let source = request(&mut oracle, "prepare", &f);
        let native = prepare_native(&f);
        assert_eq!(source["error"]["name"], "TypeError");
        assert_eq!(source["error"]["code"], "ERR_INVALID_ARG_VALUE");
        assert_eq!(
            source, native,
            "actual-source path argument validation, including exact UTF-16 error message"
        );
        assert!(
            !fs::exists(format!("{root}/fresh")).unwrap(),
            "validation precedes mkdir"
        );
    }
    assert!(pi_js_compat::node_fs::node_path_argument_error(&JsString::from("/tmp/valid-path")).is_none());
}

// Only these tests select the resident delta; frozen tests keep the common pin.
// The mandatory runner exposes verified delta-lock checkouts read-only.
fn approved_delta_oracle() -> oracle::TsOracle {
    let pin = resident_delta_pin();
    let checkout = std::path::Path::new(pin["checkout"].as_str().unwrap());
    let module = verify_resident_delta(checkout, &pin)
        .unwrap_or_else(|error| panic!("approved resident delta unavailable (no frozen fallback): {error}"));
    oracle::TsOracle::spawn_with_environment(
        "resident-session-oracle.ts",
        "packages/coding-agent/src",
        &[("PI_RESIDENT_SESSION_DELTA_MODULE", module.into_os_string())],
    )
}

#[test]
fn delta_source_created_trees_native_prepare_and_source_reader_reciprocal() {
    run_source_created_trees_native_prepare_and_source_reader_reciprocal(approved_delta_oracle(), Vec::new());
}

#[test]
fn delta_paired_actual_self_pids_cover_live_validation_without_impersonating_pi() {
    run_paired_actual_self_pids_cover_live_validation_without_impersonating_pi(approved_delta_oracle(), Vec::new());
}

#[test]
fn delta_source_owned_dead_and_live_non_pi_children_are_not_resident_runtimes() {
    run_source_owned_dead_and_live_non_pi_children_are_not_resident_runtimes(approved_delta_oracle(), Vec::new());
}

#[test]
fn delta_independent_selection_unicode_relative_types_and_paired_liveness() {
    run_independent_selection_unicode_relative_types_and_paired_liveness(approved_delta_oracle());
}

fn resident_delta_pin() -> Value {
    const MODULE: &str = "packages/coding-agent/src/core/resident-session.ts";
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/delta-lock.json"
    )))
    .expect("delta lock must be valid JSON");
    let delta = lock["deltas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|delta| delta["modules"].get(MODULE).is_some())
        .expect("resident module must have an explicit approved delta pin");
    json!({"checkout":delta["checkout"],"commit":delta["commit"],"module":MODULE,"moduleSha256":delta["modules"][MODULE]})
}

fn verify_resident_delta(checkout: &std::path::Path, pin: &Value) -> Result<std::path::PathBuf, String> {
    use sha2::{Digest, Sha256};

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("cannot verify resident delta HEAD at {}: {error}", checkout.display()))?;
    if !output.status.success() {
        return Err(format!(
            "cannot verify resident delta HEAD at {}: {}",
            checkout.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let actual = String::from_utf8_lossy(&output.stdout);
    let expected = pin["commit"].as_str().unwrap();
    if actual.trim() != expected {
        return Err(format!(
            "resident delta HEAD mismatch: expected {expected}, got {}",
            actual.trim()
        ));
    }
    let module = checkout.join(pin["module"].as_str().unwrap());
    let bytes =
        fs::read(&module).map_err(|error| format!("cannot read resident delta {}: {error}", module.display()))?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    let expected_hash = pin["moduleSha256"].as_str().unwrap();
    if hash != expected_hash {
        return Err(format!(
            "resident delta SHA256 mismatch: expected {expected_hash}, got {hash}"
        ));
    }
    Ok(module)
}

#[test]
fn resident_delta_selector_rejects_missing_checkout() {
    let root = tempfile::tempdir().unwrap();
    let pin = resident_delta_pin();
    let error = verify_resident_delta(&root.path().join("absent"), &pin).unwrap_err();
    assert!(error.contains("HEAD"), "{error}");
}

#[test]
fn resident_delta_selector_rejects_wrong_commit() {
    let pin = resident_delta_pin();
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    let error = verify_resident_delta(std::path::Path::new(lock["worktree"].as_str().unwrap()), &pin).unwrap_err();
    assert!(error.contains("HEAD mismatch"), "{error}");
}

#[test]
fn resident_delta_selector_rejects_wrong_module_hash() {
    let mut pin = resident_delta_pin();
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    pin["commit"] = lock["commit"].clone();
    let error = verify_resident_delta(std::path::Path::new(lock["worktree"].as_str().unwrap()), &pin).unwrap_err();
    assert!(error.contains("SHA256 mismatch"), "{error}");
}
