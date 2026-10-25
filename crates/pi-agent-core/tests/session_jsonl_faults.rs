#[path = "support/session_fault_fs.rs"]
mod fault_fs;
use crate::shared_oracle as oracle;
use fault_fs::{FaultFs, FaultRule, fault_message};
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::*;
use pi_agent_core::harness::types::{FileError, FileErrorCode, FileSystem, OperationFailure};
use pi_js_compat::{
    js_json as json,
    js_json::{JsObject, JsString, JsValue, parse_lossless, stringify_lossless},
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
const ISO: &str = "2023-11-14T22:13:20.000Z";

#[derive(Clone, Copy)]
struct Case {
    kind: &'static str,
    action: &'static str,
    fault: Option<FaultRule>,
}
fn base_entries() -> Vec<JsValue> {
    vec![
        json!({"type":"custom","id":"entry","parentId":null,"timestamp":ISO,"customType":"base"}),
        json!({"type":"custom","id":"tail","parentId":"entry","timestamp":ISO,"customType":"base"}),
    ]
}
fn header() -> JsValue {
    json!({"type":"session","version":3,"id":"fixture","timestamp":ISO,"cwd":"/fixture"})
}
fn append_entry() -> JsValue {
    json!({"type":"label","id":"new-label","parentId":"entry","timestamp":ISO,"targetId":"entry","label":fault_message()})
}
fn clock() -> Clock {
    Arc::new(|| ISO.into())
}
fn failure(error: SessionFailure) -> JsValue {
    let error = match error {
        OperationFailure::Returned(error) => error,
        OperationFailure::Rejected(error) => return error.error.value(),
    };
    let mut value = json!({"name":"SessionError","code":error.code.as_str(),"message":error.message});
    if let Some(cause) = error.cause {
        let cause = cause
            .downcast_ref::<FileError>()
            .expect("fixture supplies typed FileError");
        let mut record = json!({"name":cause.name,"code":cause.code.as_str(),"message":cause.message});
        if let Some(path) = &cause.path {
            record["path"] = JsValue::String(path.clone());
        }
        value["cause"] = record;
    }
    value
}
fn attempt(result: Result<(), SessionFailure>) -> JsValue {
    match result {
        Ok(()) => json!({"ok":true}),
        Err(error) => json!({"ok":false,"error":failure(error)}),
    }
}
async fn snapshot(storage: &JsonlSessionStorage) -> JsValue {
    json!({"entries":storage.get_entries().await.unwrap(),"leaf":storage.get_leaf_id().await.unwrap(),"label":storage.get_label(&"entry".into()).await.unwrap()})
}
fn files(root: &std::path::Path) -> JsValue {
    fn collect(root: &std::path::Path, current: &std::path::Path, output: &mut JsObject) {
        for item in std::fs::read_dir(current).unwrap() {
            let item = item.unwrap();
            let path = item.path();
            if path.is_dir() {
                collect(root, &path, output);
            } else {
                output.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    JsValue::String(std::fs::read_to_string(path).unwrap().into()),
                );
            }
        }
    }
    let mut output = JsObject::default();
    collect(root, root, &mut output);
    JsValue::Object(output)
}
async fn storage_case(case: Case, root: &std::path::Path, ids: IdGenerator) -> JsValue {
    let path: JsString = root.join("session.jsonl").to_str().unwrap().into();
    if case.action != "create" {
        std::fs::write(
            path.to_utf8().unwrap(),
            format!(
                "{}\n{}\n",
                stringify_lossless(&header()),
                stringify_lossless(&base_entries()[0])
            ),
        )
        .unwrap();
    }
    let fs = Arc::new(FaultFs::new(root.to_str().unwrap().into()));
    fs.arm(case.fault);
    let fs_api: Arc<dyn FileSystem> = fs.clone();
    match case.action {
        "create" => {
            json!({"attempt":attempt(JsonlSessionStorage::create_with_runtime(fs_api,path,JsonlSessionStorageCreateOptions {cwd:"/fixture".into(),session_id:"fixture".into(),parent_session_path:None},clock(),ids.clone()).await.map(|_|()))})
        }
        "open" => {
            json!({"attempt":attempt(JsonlSessionStorage::open_with_runtime(fs_api,path,clock(),ids.clone()).await.map(|_|()))})
        }
        "header" => json!({"attempt":attempt(load_jsonl_session_metadata(fs.as_ref(),&path).await.map(|_|()))}),
        "append" | "leaf" => {
            let storage = JsonlSessionStorage::open_with_runtime(fs_api, path.clone(), clock(), ids.clone())
                .await
                .unwrap();
            async fn operation(storage: &JsonlSessionStorage, action: &str) -> Result<(), SessionFailure> {
                if action == "append" {
                    storage
                        .append_entry(SessionTreeEntry::from_value(append_entry()).unwrap())
                        .await
                } else {
                    storage.set_leaf_id(None).await
                }
            }
            let result = attempt(operation(&storage, case.action).await);
            let state = snapshot(&storage).await;
            let real = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
                cwd: root.to_str().unwrap().into(),
                ..Default::default()
            }));
            let reopened = snapshot(&JsonlSessionStorage::open(real, path.clone()).await.unwrap()).await;
            let disk = std::fs::read_to_string(path.to_utf8().unwrap()).unwrap();
            let retry = attempt(operation(&storage, case.action).await);
            json!({"attempt":result,"state":state,"reopened":reopened,"diskBeforeRetry":disk,"retry":retry,"afterRetry":snapshot(&storage).await})
        }
        _ => panic!("unknown storage action"),
    }
}
async fn repository_case(case: Case, root: &std::path::Path, ids: IdGenerator) -> JsValue {
    let options = |fs: Arc<dyn FileSystem>| JsonlSessionRepoOptions {
        fs,
        sessions_root: root.join("sessions").to_str().unwrap().into(),
    };
    let real = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.to_str().unwrap().into(),
        ..Default::default()
    }));
    let seed = JsonlSessionRepo::with_runtime(options(real), clock(), ids.clone());
    let create = |id: &str| JsonlSessionCreateOptions {
        id: Some(id.into()),
        cwd: "/fixture".into(),
        parent_session_path: None,
    };
    let session = seed.create(create("fixture")).await.unwrap();
    for entry in base_entries() {
        session
            .get_storage()
            .append_entry(SessionTreeEntry::from_value(entry).unwrap())
            .await
            .unwrap();
    }
    let metadata = session.get_metadata().await.unwrap();
    let fs = Arc::new(FaultFs::new(root.to_str().unwrap().into()));
    fs.arm(case.fault);
    let repo = JsonlSessionRepo::with_runtime(options(fs), clock(), ids.clone());
    let result = match case.action {
        "create" => repo.create(create("new")).await.map(|_| ()),
        "open" => repo.open(&metadata).await.map(|_| ()),
        "delete" => repo.delete(&metadata).await,
        "list" => repo.list(JsonlSessionListOptions::default()).await.map(|_| ()),
        "listCwd" => repo
            .list(JsonlSessionListOptions {
                cwd: Some("/fixture".into()),
            })
            .await
            .map(|_| ()),
        "fork" => repo
            .fork(
                &metadata,
                JsonlSessionForkOptions {
                    create: create("fork"),
                    entry_id: None,
                    position: ForkPosition::Before,
                },
            )
            .await
            .map(|_| ()),
        _ => panic!("unknown repository action"),
    };
    json!({"attempt":attempt(result)})
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (kind, action, method, nth) in [
        ("storage", "create", "writeFile", 1),
        ("storage", "open", "readTextFile", 1),
        ("storage", "header", "readTextLines", 1),
        ("storage", "append", "appendFile", 1),
        ("storage", "leaf", "appendFile", 1),
        ("repo", "create", "absolutePath", 1),
        ("repo", "create", "joinPath", 1),
        ("repo", "create", "joinPath", 3),
        ("repo", "create", "createDir", 1),
        ("repo", "create", "writeFile", 1),
        ("repo", "open", "exists", 1),
        ("repo", "open", "readTextFile", 1),
        ("repo", "delete", "remove", 1),
        ("repo", "list", "exists", 1),
        ("repo", "list", "exists", 2),
        ("repo", "list", "listDir", 1),
        ("repo", "list", "listDir", 2),
        ("repo", "list", "readTextLines", 1),
        ("repo", "listCwd", "joinPath", 1),
        ("repo", "listCwd", "exists", 1),
        ("repo", "fork", "exists", 1),
        ("repo", "fork", "readTextFile", 1),
        ("repo", "fork", "createDir", 1),
        ("repo", "fork", "writeFile", 1),
        ("repo", "fork", "appendFile", 1),
        ("repo", "fork", "appendFile", 2),
    ] {
        for code in [FileErrorCode::NotFound, FileErrorCode::PermissionDenied] {
            cases.push(Case {
                kind,
                action,
                fault: Some(FaultRule {
                    method,
                    nth,
                    after: false,
                    rejected: false,
                    code,
                }),
            });
        }
        if method != "joinPath" {
            cases.push(Case {
                kind,
                action,
                fault: Some(FaultRule {
                    method,
                    nth,
                    after: false,
                    rejected: true,
                    code: FileErrorCode::PermissionDenied,
                }),
            });
        }
        if ["writeFile", "appendFile", "remove"].contains(&method) {
            cases.push(Case {
                kind,
                action,
                fault: Some(FaultRule {
                    method,
                    nth,
                    after: true,
                    rejected: false,
                    code: FileErrorCode::PermissionDenied,
                }),
            });
        }
    }
    for (kind, action) in [
        ("storage", "create"),
        ("storage", "open"),
        ("storage", "header"),
        ("storage", "append"),
        ("storage", "leaf"),
        ("repo", "create"),
        ("repo", "open"),
        ("repo", "delete"),
        ("repo", "list"),
        ("repo", "listCwd"),
        ("repo", "fork"),
    ] {
        cases.push(Case {
            kind,
            action,
            fault: None,
        });
    }
    cases
}

#[tokio::test]
async fn jsonl_storage_and_repository_filesystem_failures_preserve_errors_disk_and_memory() {
    let mut samples = oracle::TsOracle::spawn(
        "tree-session-oracle.ts",
        "packages/agent-core/src/harness/session/session.ts",
    );
    let response =
        samples.request(&serde_json::json!({"wire":stringify_lossless(&json!({"kind":"uuidSamples","count":512}))}));
    let response = parse_lossless(response["wire"].as_str().unwrap()).unwrap();
    let queue = Arc::new(Mutex::new(
        response["value"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap().clone())
            .collect::<VecDeque<_>>(),
    ));
    let ids: IdGenerator = Arc::new(move || {
        queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("fixture has enough frozen UUID samples")
    });
    let mut oracle = oracle::TsOracle::spawn(
        "session-jsonl-fault-oracle.ts",
        "packages/agent-core/src/harness/session/jsonl-storage.ts",
    );
    for case in cases() {
        let root = tempfile::tempdir().unwrap();
        let mut actual = if case.kind == "storage" {
            storage_case(case, root.path(), ids.clone()).await
        } else {
            repository_case(case, root.path(), ids.clone()).await
        };
        actual["files"] = files(root.path());
        std::fs::remove_dir_all(root.path()).unwrap();
        std::fs::create_dir(root.path()).unwrap();
        let fault = case
            .fault
            .map(|rule| json!({"method":rule.method,"nth":rule.nth,"after":rule.after,"rejected":rule.rejected,"code":rule.code.as_str()}));
        let input = json!({"kind":case.kind,"action":case.action,"root":root.path(),"fault":fault,"message":fault_message(),"header":header(),"base":base_entries(),"append":append_entry()});
        let expected = oracle.request(&input);
        assert_eq!(
            actual,
            expected,
            "kind={} action={} fault={}",
            case.kind,
            case.action,
            stringify_lossless(&input["fault"])
        );
    }
}
