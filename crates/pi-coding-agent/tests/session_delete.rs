#[path = "support/oracle.rs"]
mod oracle;
use oracle::TsOracle;
use pi_coding_agent::core::session_delete::delete_session_tree;
use serde_json::json;

#[tokio::test]
async fn delete_worker() {
    let Ok(path) = std::env::var("DELETE_FIXTURE") else {
        return;
    };
    let result = delete_session_tree(&path, None).await;
    std::fs::write(format!("{path}.result"), serde_json::to_vec(&result).unwrap()).unwrap();
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
