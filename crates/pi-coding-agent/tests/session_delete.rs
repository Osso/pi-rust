use crate::shared_oracle as oracle;
use oracle::TsOracle;
use pi_coding_agent::core::session_delete::delete_session_tree;
use serde_json::json;

#[tokio::test]
async fn delete_worker() {
    let Ok(path) = std::env::var("DELETE_FIXTURE") else {
        return;
    };
    let db = std::env::var("DELETE_DB").ok();
    let path_text = pi_js_compat::js_json::JsString::from(path.as_str());
    let db = db.map(pi_js_compat::js_json::JsString::from);
    let result = delete_session_tree(&path_text, db.as_ref()).await;
    let output = if std::env::var_os("DELETE_UTF16").is_some() {
        let units: Vec<u16> = match &result {
            pi_coding_agent::core::session_delete::SessionDeleteResult::Failure { error, .. } => error.units().to_vec(),
            _ => panic!("expected deletion failure"),
        };
        json!({"json":serde_json::to_string(&result).unwrap(),"units":units})
    } else {
        serde_json::to_value(&result).unwrap()
    };
    std::fs::write(format!("{path}.result"), serde_json::to_vec(&output).unwrap()).unwrap();
}

#[test]
fn deletion_matches_frozen_for_trash_unlink_missing_and_partial_failure() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let trash = bin.join("trash");
    std::fs::write(&trash, "#!/usr/bin/python3\nimport os,sys\np=sys.argv[2:]\nassert sum(len(x.encode())+9 for x in p)<=65536\nfor x in p:\n if 'blocked' in x: continue\n if os.path.isfile(x): os.unlink(x)\nif any('blocked' in x for x in p):\n print('intentional partial trash failure',file=sys.stderr)\n sys.exit(1)\n").unwrap();
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();
    let oracle_path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut oracle = TsOracle::spawn_with_environment(
        "session-storage-oracle.ts",
        "session-names:packages/coding-agent/src/core",
        &[("PATH", oracle_path.into())],
    );
    for name in ["standalone", "missing", "blocked", "unlink"] {
        let path = root.path().join(name);
        if name == "blocked" {
            std::fs::create_dir(&path).unwrap();
        } else if name != "missing" {
            std::fs::write(&path, "fixture").unwrap();
        }
        let response = oracle.request(&json!({"id": name, "op":"delete", "path":path}));
        assert_eq!(response["ok"], true, "{response}");
        if name == "standalone" || name == "unlink" {
            std::fs::write(&path, "fixture").unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "delete_worker"])
            .env("DELETE_FIXTURE", &path)
            .env(
                "PATH",
                if name == "unlink" {
                    root.path().join("no-bin")
                } else {
                    bin.clone()
                },
            )
            .status()
            .unwrap();
        assert!(status.success());
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(format!("{}.result", path.display())).unwrap()).unwrap();
        if name == "blocked" {
            assert_eq!(actual["ok"], false);
            assert!(
                actual["error"]
                    .as_str()
                    .unwrap()
                    .contains("intentional partial trash failure")
            );
            assert!(
                response["output"]["error"]
                    .as_str()
                    .unwrap()
                    .contains("intentional partial trash failure")
            );
        } else {
            let expected = if name == "unlink" {
                json!({"ok":true,"method":"unlink"})
            } else {
                response["output"].clone()
            };
            assert_eq!(actual, expected);
            assert!(!path.exists());
        }
    }
}

fn write_metadata(db: &str, path: &std::path::Path, parent: Option<&std::path::Path>) {
    use pi_coding_agent::core::session_control_db::{WritableSessionMetadata, write_session_metadata};
    let row:WritableSessionMetadata = serde_json::from_value(json!({"sessionPath":path,"parentSessionPath":parent,"isSubagent":parent.is_some(),"id":path.file_name().unwrap().to_str().unwrap(),"cwd":path.parent().unwrap(),"createdAt":"2026-10-21T00:00:00.000Z","modifiedAt":"2026-10-21T00:00:00.000Z","messageCount":1,"firstMessage":"fixture"})).unwrap();
    write_session_metadata(db, &row).unwrap();
}

#[test]
fn frozen_tree_deletion_batches_long_paths_and_cleans_partial_metadata() {
    use pi_coding_agent::core::session_control_db::list_session_metadata;
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let trash = bin.join("trash");
    std::fs::write(&trash,"#!/usr/bin/python3\nimport os,sys,time\np=sys.argv[2:]\nassert sum(len(x.encode())+9 for x in p)<=65536\ntime.sleep(0.04)\npartial=any('blocked' in x for x in p)\nfor x in p:\n if partial and ('blocked' in x or x.endswith('root.jsonl')): continue\n if os.path.isfile(x): os.unlink(x)\nif partial:\n print('intentional partial trash failure',file=sys.stderr)\n sys.exit(1)\n").unwrap();
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();
    let oracle_path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut oracle = TsOracle::spawn_with_environment(
        "session-storage-oracle.ts",
        "packages/coding-agent/src/core",
        &[("PATH", oracle_path.into())],
    );
    for scenario in ["tree", "partial", "long", "cleanup-error"] {
        let dir = temp.path().join(scenario);
        std::fs::create_dir(&dir).unwrap();
        let db = dir.join("control.sqlite").display().to_string();
        let root = dir.join(if scenario == "tree" {
            "root.jsonl.zst"
        } else {
            "root.jsonl"
        });
        let child = dir.join(if scenario == "partial" {
            "blocked.jsonl"
        } else {
            "child.jsonl.zst"
        });
        let removed = dir.join("removed.jsonl");
        let unrelated = dir.join("unrelated.jsonl");
        let missing = dir.join("missing.jsonl");
        let root_alias = root.with_extension("");
        let child_alias = child.with_extension("");
        let root_parent = if scenario == "tree" {
            root_alias.as_path()
        } else {
            root.as_path()
        };
        let child_parent = if scenario == "partial" {
            child.as_path()
        } else {
            child_alias.as_path()
        };
        let mut fixtures = vec![
            (root.clone(), None),
            (child.clone(), Some(root_parent.to_path_buf())),
            (removed.clone(), Some(child_parent.to_path_buf())),
            (unrelated.clone(), None),
            (missing.clone(), Some(root_parent.to_path_buf())),
        ];
        if scenario == "long" {
            fixtures.extend((0..700).map(|i| (dir.join(format!("{i}-{}.jsonl", "é".repeat(100))), Some(root.clone()))));
        }
        let populate = || {
            for (path, parent) in &fixtures {
                if path == &missing {
                } else if scenario == "partial" && path == &child {
                    if !path.exists() {
                        std::fs::create_dir(path).unwrap();
                    }
                } else {
                    std::fs::write(path, "fixture").unwrap();
                }
                write_metadata(&db, path, parent.as_deref());
            }
        };
        populate();
        if scenario == "cleanup-error" {
            rusqlite::Connection::open(&db).unwrap().execute_batch("CREATE TRIGGER prevent_metadata_cleanup BEFORE DELETE ON session_metadata BEGIN SELECT RAISE(ABORT, 'fixture cleanup denied'); END;").unwrap();
        }
        let expected = oracle.request(&json!({"id":scenario,"op":"delete","path":root,"db":db}));
        assert_eq!(expected["ok"], true, "{expected}");
        let expected_rows: Vec<_> = list_session_metadata(&db)
            .unwrap()
            .iter()
            .map(|r| r.session_path.clone())
            .collect();
        populate();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "delete_worker"])
            .env("DELETE_FIXTURE", &root)
            .env("DELETE_DB", &db)
            .env("PATH", &bin)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(format!("{}.result", root.display())).unwrap()).unwrap();
        assert_eq!(actual, expected["output"], "{scenario}");
        let actual_rows: Vec<_> = list_session_metadata(&db)
            .unwrap()
            .iter()
            .map(|r| r.session_path.clone())
            .collect();
        assert_eq!(actual_rows, expected_rows, "{scenario}");
        assert!(unrelated.exists());
        assert!(!removed.exists());
        if scenario == "cleanup-error" {
            assert_eq!(actual_rows.len(), fixtures.len());
            assert_eq!(actual["ok"], false);
            assert!(!root.exists());
        } else if scenario == "partial" {
            assert!(root.exists());
            assert!(child.exists());
            assert_eq!(actual_rows.len(), 3);
        } else {
            assert!(!root.exists());
            assert!(!child.exists());
            assert_eq!(actual_rows, vec![unrelated.display().to_string()]);
        }
    }
}

#[test]
fn trash_without_shebang_matches_node_execution() {
    assert_trash_diagnostic_matches_frozen("printf '%s\\n' \"$0\" \"$@\" > \"$2.args\"\nexit 7\n", false);
}

#[test]
fn trash_split_surrogate_preserves_public_utf16() {
    assert_trash_diagnostic_matches_frozen(
        "#!/usr/bin/python3\nimport sys;sys.stderr.write('x'*199+'😀');sys.exit(1)\n",
        true,
    );
}

fn assert_trash_diagnostic_matches_frozen(script: &str, utf16: bool) {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let trash = root.path().join("trash");
    std::fs::write(&trash, script).unwrap();
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = root.path().join("blocked space ' $(not-a-command)");
    std::fs::create_dir(&path).unwrap();
    let mut oracle = TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let expected = oracle.request(&json!({"op":"delete","path":path,"pathEnv":root.path(),"utf16":utf16}));
    assert_eq!(expected["ok"], true, "{expected}");
    let args_path = format!("{}.args", path.display());
    let expected_args = if utf16 {
        None
    } else {
        let args = std::fs::read_to_string(&args_path).unwrap();
        assert_eq!(args, format!("{}\n--\n{}\n", trash.display(), path.display()));
        std::fs::remove_file(&args_path).unwrap();
        Some(args)
    };
    let mut worker = std::process::Command::new(std::env::current_exe().unwrap());
    worker
        .args(["--exact", "delete_worker"])
        .env("DELETE_FIXTURE", &path)
        .env("PATH", root.path());
    if utf16 {
        worker.env("DELETE_UTF16", "1");
    }
    let output = worker.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let actual: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{}.result", path.display())).unwrap()).unwrap();
    if utf16 {
        let units = expected["output"]["units"].as_array().unwrap();
        assert_eq!(&units[units.len() - 3..], &[json!(120), json!(55357), json!(41)]);
    } else {
        assert!(
            expected["output"]["error"]
                .as_str()
                .unwrap()
                .ends_with("(trash: trash exited with 7)")
        );
    }
    assert_eq!(actual, expected["output"]);
    if let Some(expected_args) = expected_args {
        assert_eq!(std::fs::read_to_string(args_path).unwrap(), expected_args);
    }
}

#[test]
fn trash_error_messages_match_frozen() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let mut oracle = TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    for (name, body) in [
        ("permission", "pass"),
        ("exit", "import sys;sys.exit(7)"),
        ("signal", "import os,signal;os.kill(os.getpid(),signal.SIGTERM)"),
        (
            "long-stderr",
            "import sys;print('x'*250+'\\nsecond line',file=sys.stderr);sys.exit(1)",
        ),
        ("success", "pass"),
    ] {
        let dir = root.path().join(name);
        std::fs::create_dir(&dir).unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let trash = bin.join("trash");
        std::fs::write(&trash, format!("#!/usr/bin/python3\n{body}\n")).unwrap();
        std::fs::set_permissions(
            &trash,
            std::fs::Permissions::from_mode(if name == "permission" { 0o644 } else { 0o755 }),
        )
        .unwrap();
        let path = dir.join("target");
        if name == "success" {
            std::fs::write(&path, "fixture").unwrap();
        } else {
            std::fs::create_dir(&path).unwrap();
        }
        let expected = oracle.request(&json!({"id":name,"op":"delete","path":path,"pathEnv":bin}));
        assert_eq!(expected["ok"], true, "{expected}");
        if name == "success" {
            std::fs::write(&path, "fixture").unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "delete_worker"])
            .env("DELETE_FIXTURE", &path)
            .env("PATH", &bin)
            .output()
            .unwrap();
        assert!(output.status.success());
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(format!("{}.result", path.display())).unwrap()).unwrap();
        assert_eq!(actual, expected["output"], "{name}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
