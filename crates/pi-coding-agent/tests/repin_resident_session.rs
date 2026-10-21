//! Actual-source filesystem/metadata interoperability, not a runtime impersonation.
#[path = "support/oracle.rs"]
mod oracle;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use pi_coding_agent::core::resident_session::{ResidentRole, prepare_resident_session_file};
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions, WritableSessionMetadata, list_session_metadata,
    register_runtime_mailbox_listener, write_session_metadata,
};
use serde_json::{Value, json};

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn("resident-session-oracle.ts", "packages/coding-agent/src")
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
        "unicode" => {
            f["entries"].as_array_mut().unwrap().extend([
                json!({"path": format!("{dir}/\u{10000}_{role}.jsonl")}),
                json!({"path": format!("{dir}/\u{e000}_{role}.jsonl")}),
            ]);
        }
        "relative" => {
            let cwd = std::env::current_dir().unwrap();
            let relative = pi_coding_agent::utils::paths::relative_path(&cwd.display().to_string(), root);
            f["agentDir"] = json!(format!("{relative}/absent/../agent/./"));
        }
        "self-existing" | "self-missing" | "self-unmatched" | "self-directory" | "self-relative" | "self-outside"
        | "self-extension" | "self-symlink-dir" | "self-agent" | "no-path" | "dead" | "nonpi" => {
            let path = match case {
                "self-existing" | "self-agent" | "dead" | "nonpi" => older,
                "self-missing" => format!("{dir}/missing_{role}.jsonl"),
                "self-unmatched" => format!("{dir}/unmatched.jsonl"),
                "self-directory" => format!("{dir}/z_{role}.jsonl"),
                "self-relative" => {
                    let cwd = std::env::current_dir().unwrap();
                    pi_coding_agent::utils::paths::relative_path(
                        &cwd.display().to_string(),
                        &format!("{dir}/./sub/../100_{role}.jsonl"),
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
                agent_id: listener["agentId"].as_str().map(str::to_owned),
            },
            std::process::id(),
            listener["path"].as_str(),
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
    match prepare_resident_session_file(f["agentDir"].as_str().unwrap(), role, f["db"].as_str().unwrap()) {
        Ok(result) => json!({"ok": true, "value": result}),
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}

fn normalized(value: Value, root: &str) -> Value {
    serde_json::from_str(&serde_json::to_string(&value).unwrap().replace(root, "ROOT")).unwrap()
}

fn restore_permissions(f: &Value) {
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
fn source_created_trees_native_prepare_and_source_reader_reciprocal() {
    let mut oracle = oracle();
    for role in ["architect", "supervisor"] {
        for case in ["empty", "ordinary", "unicode", "relative"] {
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
            assert_eq!(outputs[0], outputs[1], "{role}/{case}");
            let path = outputs[0]["prepared"]["value"]["sessionPath"].as_str().unwrap();
            let expected = match case {
                "empty" => format!("{role}.jsonl"),
                "unicode" => format!("\u{e000}_{role}.jsonl"),
                _ => format!("900_{role}.jsonl"),
            };
            assert!(path.ends_with(&expected), "{case}: {path}");
        }
    }
}

#[test]
fn paired_actual_self_pids_cover_live_validation_without_impersonating_pi() {
    let mut oracle = oracle();
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
            assert_eq!(
                outputs[0], outputs[1],
                "{role}/{case}: paired self-PID branches, different real processes"
            );
        }
    }
}

#[test]
fn source_owned_dead_and_live_non_pi_children_are_not_resident_runtimes() {
    let mut oracle = oracle();
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
        assert_eq!(outputs[0], outputs[1], "{case}");
        assert!(
            outputs[0]["prepared"]["value"]["sessionPath"]
                .as_str()
                .unwrap()
                .ends_with("900_supervisor.jsonl")
        );
    }
}

#[test]
fn actual_source_filesystem_errors_and_metadata_before_unlink_order() {
    let mut oracle = oracle();
    for case in ["mkdir-error", "ancestor-error", "scandir-error", "unlink-error"] {
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
            let message = if native {
                outcome["error"].clone()
            } else {
                outcome["error"]["message"].clone()
            };
            messages.push(normalized(message, &root));
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

// File/path/liveness proof is independent of the explicitly failing metadata regex
// fixtures above. All metadata rows, including backslash cases, remain in these trees.
#[test]
fn independent_selection_unicode_relative_types_and_paired_liveness() {
    let mut oracle = oracle();
    for role in ["architect", "supervisor"] {
        for case in [
            "empty",
            "ordinary",
            "unicode",
            "relative",
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
                let entries = success(&mut oracle, "inspect", &f)["entries"].clone();
                outputs.push(normalized(json!({"prepared": prepared, "entries": entries}), &root));
            }
            assert_eq!(outputs[0], outputs[1], "{role}/{case}: concrete paths/files/types");
            let expected = match case {
                "empty" => format!("{role}.jsonl"),
                "unicode" => format!("\u{e000}_{role}.jsonl"),
                "self-existing" | "self-relative" => format!("100_{role}.jsonl"),
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
