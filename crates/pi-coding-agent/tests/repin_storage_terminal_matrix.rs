use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::session_control_db::{
    AgentDetails, CommitMultiAgentTerminalMutationInput, CommitMultiAgentTerminalMutationResult,
    CreateMultiAgentChildWithRuntimeOwnershipInput, CreateMultiAgentChildWithRuntimeOwnershipResult,
    MultiAgentRuntimeOwnerIdentity, commit_multi_agent_terminal_mutation,
    create_multi_agent_child_with_runtime_ownership, read_multi_agent_agent,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::terminal_outbox_delivery::{
    DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections,
};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsValue, from_value};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    directory: tempfile::TempDir,
    db: String,
    path: String,
    now: String,
}

fn source_call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let response = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    response["output"][0]["value"].clone()
}

fn fixture(ts: &mut oracle::TsOracle) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let db = directory.path().join("control.sqlite").to_str().unwrap().to_owned();
    let created = ts.request(&json!({"id":1,"op":"create","dir":directory.path(),"db":db}));
    assert_eq!(created["ok"], true, "{created}");
    Fixture {
        directory,
        db,
        path: created["output"]["path"].as_str().unwrap().to_owned(),
        now: pi_coding_agent::utils::iso_time::now_iso_string(),
    }
}

fn seed_terminal(f: &Fixture, ts: &mut oracle::TsOracle, id: &str, lifecycle: &str, source_first: bool) -> Value {
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    let owner = json!({"sessionId":"fixture-owner","agentId":null});
    let agent = json!({"id":id,"parentId":"main","displayName":"Worker 漢😀","agentType":"worker",
        "lifecycle":"running","revision":1,"createdAt":f.now,"updatedAt":f.now,"cwd":"/fixture",
        "permission":{"policy":"on-request","narrowed":true}});
    let creation = json!({"sessionPath":f.path,"agentId":id,"processIdentity":identity,
        "owner":owner,"nowIso":f.now,"agent":agent});
    let terminal = json!({"sessionPath":f.path,"agentId":id,"processIdentity":identity,
        "owner":owner,"terminalLifecycle":lifecycle,"eventKind":lifecycle,
        "agentDetails":{"result":{"summary":"Result 漢😀"}},"updatedAt":f.now});
    if source_first {
        assert_eq!(
            source_call(ts, "createMultiAgentChildWithRuntimeOwnership", json!([f.db, creation]))["ok"],
            true
        );
        assert_eq!(
            source_call(ts, "commitMultiAgentTerminalMutation", json!([f.db, terminal]))["ok"],
            true
        );
    } else {
        let owner = MultiAgentRuntimeOwnerIdentity {
            session_id: "fixture-owner".into(),
            agent_id: None,
        };
        let creation = CreateMultiAgentChildWithRuntimeOwnershipInput {
            session_path: f.path.as_str().into(),
            agent_id: id.into(),
            process_identity: identity.clone(),
            owner: owner.clone(),
            now_iso: f.now.clone(),
            agent: from_value(&JsValue::from(agent)).unwrap(),
        };
        let created = create_multi_agent_child_with_runtime_ownership(&f.db, &creation).unwrap();
        assert!(
            matches!(created, CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }),
            "{created:?}"
        );
        let terminal = CommitMultiAgentTerminalMutationInput {
            session_path: f.path.as_str().into(),
            agent_id: id.into(),
            process_identity: identity,
            owner,
            terminal_lifecycle: lifecycle.into(),
            event_kind: lifecycle.into(),
            agent_details: AgentDetails {
                error: None,
                result: Some(JsValue::from(json!({"summary":"Result 漢😀"}))),
            },
            updated_at: f.now.clone(),
        };
        let committed = commit_multi_agent_terminal_mutation(&f.db, &terminal).unwrap();
        assert!(
            matches!(
                committed,
                CommitMultiAgentTerminalMutationResult::Ok { terminal_revision: 2 }
            ),
            "{committed:?}"
        );
    }
    serde_json::to_value(read_multi_agent_agent(&f.db, &f.path, id).unwrap().unwrap()).unwrap()
}

fn deliver_native(f: &Fixture, throw_at: Option<usize>) -> Value {
    let mut session = SessionManager::open(&f.path.as_str().into(), None, None).unwrap();
    session
        .set_metadata_control_db_path(Some(&f.db.as_str().into()), Default::default())
        .unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&session));
    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = Arc::clone(&calls);
    let now = f.now.clone();
    let result = deliver_terminal_outbox_projections(&DeliverTerminalOutboxOptions {
        artifact_root: Some(f.directory.path().to_str().unwrap().into()),
        claim_id: "fixture".into(),
        control_db_path: f.db.as_str().into(),
        store: Arc::clone(&store),
        now: Arc::new(move || {
            let call = clock_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if throw_at == Some(call) {
                return Err(CallbackError::from(
                    NativeNodeError::from_constructor(NativeErrorClass::TypeError, "clock failed")
                        .with_property("code", JsValue::String("FIXTURE".into())),
                ));
            }
            Ok(now.as_str().into())
        }),
    });
    let agents = serde_json::to_value(store.list_agents()).unwrap();
    match result {
        Ok(delivered) => json!({"ok":true,"delivered":delivered,"calls":calls.load(Ordering::SeqCst),"agents":agents}),
        Err(error) => {
            let AbortReason::Error(error) = error.thrown_value() else {
                panic!("fixture expects Error")
            };
            json!({"ok":false,"error":error.value().to_serde().unwrap(),"calls":calls.load(Ordering::SeqCst),"agents":agents})
        }
    }
}

fn deliver_source(f: &Fixture, ts: &mut oracle::TsOracle, throw_at: Option<usize>) -> Value {
    let response = ts.request(&json!({"id":1,"op":"project","db":f.db,"path":f.path,
        "dir":f.directory.path(),"clock":if throw_at.is_some(){"throw-error"}else{f.now.as_str()},
        "clockValue":f.now,"throwAt":throw_at}));
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

fn connection(f: &Fixture) -> rusqlite::Connection {
    pi_coding_agent::core::sqlite::create_sqlite_database(&f.db).unwrap()
}

fn rows(f: &Fixture) -> Vec<Value> {
    let db = connection(f);
    let mut query=db.prepare("SELECT agent_id,status,attempt_count,claim_id,last_error FROM multi_agent_terminal_outbox ORDER BY agent_id").unwrap();
    query.query_map([],|row|Ok(json!({"agentId":row.get::<_,String>(0)?,"status":row.get::<_,String>(1)?,
        "attempts":row.get::<_,i64>(2)?,"claimId":row.get::<_,Option<String>>(3)?,"error":row.get::<_,Option<String>>(4)?})))
        .unwrap().collect::<Result<Vec<_>,_>>().unwrap()
}

fn restore_outbox(f: &Fixture, id: &str, attempts: i64) {
    connection(f)
        .execute(
            "INSERT OR REPLACE INTO multi_agent_terminal_outbox
        (session_path,agent_id,terminal_revision,event_kind,status,attempt_count,updated_at)
        VALUES (?, ?, 2, 'completed', 'pending', ?, ?)",
            rusqlite::params![f.path, id, attempts, f.now],
        )
        .unwrap();
}

fn seed_old_artifact(f: &Fixture, ts: &mut oracle::TsOracle) -> std::path::PathBuf {
    let stem = std::path::Path::new(&f.path).file_stem().unwrap();
    let directory = f.directory.path().join("detached-jobs").join(stem).join("old-artifact");
    let output = directory.join("output.log");
    source_call(
        ts,
        "bootstrapMultiAgentAgent",
        json!([f.db,f.path,"old-artifact",{
        "id":"old-artifact","parentId":"main","displayName":"Old job","agentType":"background",
        "lifecycle":"completed","revision":2,"createdAt":"2020-01-01T00:00:00.000Z","updatedAt":"2020-01-01T00:00:00.000Z",
        "cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},
        "result":{"summary":"done","fileRefs":[{"path":output,"label":"Bash output"}]}}]),
    );
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(output, b"expired output").unwrap();
    directory
}

#[test]
fn reciprocal_terminal_publication_ack_and_real_artifact_cleanup() {
    let mut source = oracle::TsOracle::spawn("storage-terminal-outbox-oracle.mts", "packages/coding-agent/src/core");
    let mut control = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let f = fixture(&mut source);
    for source_first in [true, false] {
        for lifecycle in ["completed", "failed", "aborted"] {
            let id = format!("{}-{lifecycle}", if source_first { "source" } else { "native" });
            let expected = seed_terminal(&f, &mut control, &id, lifecycle, source_first);
            let artifact = seed_old_artifact(&f, &mut control);
            let result = if source_first {
                deliver_native(&f, None)
            } else {
                deliver_source(&f, &mut source, None)
            };
            assert_eq!(
                JsValue::from(result),
                JsValue::from(json!({"ok":true,"delivered":1,"calls":3,"agents":[expected.clone()]}))
            );
            assert!(rows(&f).is_empty(), "delivery acknowledges only the claimed row");
            if artifact.exists() {
                let references = pi_coding_agent::core::detached_job_cleanup::read_linux_process_references().unwrap();
                let retained: Vec<_> = references
                    .into_iter()
                    .flatten()
                    .filter(|path| path.starts_with(artifact.to_str().unwrap()))
                    .collect();
                eprintln!(
                    "retained artifact {artifact:?}, source_first={source_first}, lifecycle={lifecycle}, references={retained:?}"
                );
            }
            assert!(!artifact.exists(), "finally must run actual detached artifact cleanup");
            assert_eq!(
                source_call(&mut control, "readMultiAgentAgent", json!([f.db, f.path, id])),
                expected
            );
            assert_eq!(
                serde_json::to_value(read_multi_agent_agent(&f.db, &f.path, &id).unwrap().unwrap()).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn reciprocal_mismatch_retry_poison_and_acknowledgement_conflict() {
    let mut source = oracle::TsOracle::spawn("storage-terminal-outbox-oracle.mts", "packages/coding-agent/src/core");
    let mut control = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for scenario in ["missing", "revision", "running", "ack-conflict"] {
        let f = fixture(&mut source);
        seed_terminal(&f, &mut control, "child", "completed", true);
        let db = connection(&f);
        match scenario {
            "missing" => {
                db.execute("DELETE FROM multi_agent_agents WHERE agent_id='child'", [])
                    .unwrap();
            }
            "revision" => {
                db.execute(
                    "UPDATE multi_agent_agents SET data=json_set(data,'$.revision',99) WHERE agent_id='child'",
                    [],
                )
                .unwrap();
            }
            "running" => {
                db.execute(
                    "UPDATE multi_agent_agents SET data=json_set(data,'$.lifecycle','running') WHERE agent_id='child'",
                    [],
                )
                .unwrap();
            }
            "ack-conflict" => db
                .execute_batch(
                    "CREATE TRIGGER peer_claim BEFORE DELETE ON multi_agent_terminal_outbox BEGIN
                UPDATE multi_agent_terminal_outbox SET claim_id='peer' WHERE rowid=OLD.rowid;
                SELECT RAISE(IGNORE); END;",
                )
                .unwrap(),
            _ => unreachable!(),
        }
        drop(db);
        for attempts in [0, 4] {
            restore_outbox(&f, "child", attempts);
            let expected = deliver_source(&f, &mut source, None);
            let expected_rows = rows(&f);
            restore_outbox(&f, "child", attempts);
            let actual = deliver_native(&f, None);
            assert_eq!(actual, expected, "{scenario} attempt{attempts}");
            assert_eq!(rows(&f), expected_rows, "{scenario} attempt{attempts}");
            assert_eq!(actual["ok"], false);
            let state = &expected_rows[0];
            if scenario == "ack-conflict" {
                assert_eq!(state["status"], "claimed");
                assert_eq!(state["claimId"], "peer");
            } else {
                assert_eq!(state["status"], if attempts == 4 { "poisoned" } else { "pending" });
                assert_eq!(state["attempts"], attempts + 1);
            }
        }
    }
}

#[test]
fn cleanup_finally_runs_after_later_failure_and_preserves_original_clock_throw() {
    let mut source = oracle::TsOracle::spawn("storage-terminal-outbox-oracle.mts", "packages/coding-agent/src/core");
    let mut control = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for throw_at in [2, 3] {
        let f = fixture(&mut source);
        let expected_agent = seed_terminal(&f, &mut control, "child", "completed", true);
        let artifact = seed_old_artifact(&f, &mut control);
        let expected = deliver_source(&f, &mut source, Some(throw_at));
        if throw_at == 2 {
            assert!(!artifact.exists(), "earlier clock throw still runs cleanup");
        } else {
            assert!(artifact.exists(), "cleanup clock failure occurs before cleanup");
        }
        restore_outbox(&f, "child", 0);
        std::fs::create_dir_all(&artifact).unwrap();
        std::fs::write(artifact.join("output.log"), b"expired output").unwrap();
        let actual = deliver_native(&f, Some(throw_at));
        assert_eq!(actual, expected);
        assert_eq!(
            actual["error"],
            json!({"name":"TypeError","message":"clock failed","code":"FIXTURE"})
        );
        assert_eq!(actual["agents"], json!([expected_agent]));
        assert_eq!(actual["calls"], 3);
        assert!(
            rows(&f).is_empty(),
            "the earlier projection was acknowledged before failure"
        );
        assert_eq!(artifact.exists(), throw_at == 3);
    }
}
