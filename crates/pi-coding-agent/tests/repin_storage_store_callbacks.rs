use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::{
    AgentMetadataCommandResult, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
    RecordOutboundSessionMessageInput,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn callback_failure(value: &Value, typed: bool) -> CallbackError {
    if typed {
        CallbackError::from(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, "callback failed")
                .with_property("code", JsValue::String("FIXTURE".into())),
        )
    } else {
        CallbackError::from_rejection(AbortReason::Value(JsValue::from(value)))
    }
}

fn captured(result: Result<JsValue, CallbackError>) -> Value {
    match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => {
            let value = match error.thrown_value() {
                AbortReason::Value(value) => value,
                AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
            };
            json!({"ok":false,"error":value})
        }
    }
}

fn call(source: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = source.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

#[test]
fn abort_callback_throw_removes_handler_once_and_preserves_original_value() {
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    for (failure, typed) in [
        (Value::Null, false),
        (json!("failed 漢😀"), false),
        (json!({"detail":"callback"}), false),
        (Value::Null, true),
    ] {
        let expected = call(&mut source, json!({"op":"abort","failure":failure,"typed":typed}));
        let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let called = Arc::clone(&calls);
        let error = callback_failure(&failure, typed);
        let _registration = store.register_agent_abort_handler(&"child".into(), move || {
            called.fetch_add(1, Ordering::SeqCst);
            Err(error.clone())
        });
        let first = captured(store.abort_agent_handle(&"child".into()).map(JsValue::Bool));
        assert_eq!(
            json!({"first":first,"calls":calls.load(Ordering::SeqCst),"second":store.abort_agent_handle(&"child".into()).unwrap()}),
            expected
        );
    }
}

#[test]
fn clock_callback_errors_and_partial_state_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    let mut case = 0;
    for operation in ["restore", "outbound"] {
        for throw_at in [1, 2] {
            for (failure, typed) in [
                (Value::Null, false),
                (json!("failed 漢😀"), false),
                (json!({"detail":"callback"}), false),
                (Value::Null, true),
            ] {
                let source_dir = root.path().join(format!("source{case}"));
                let native_dir = root.path().join(format!("native{case}"));
                case += 1;
                std::fs::create_dir(&source_dir).unwrap();
                std::fs::create_dir(&native_dir).unwrap();
                let now = pi_coding_agent::utils::iso_time::now_iso_string();
                let expected = call(
                    &mut source,
                    json!({"op":operation,"dir":source_dir,"db":source_dir.join("control.sqlite"),"count":2,"now":now,"throwAt":throw_at,"failure":failure,"typed":typed}),
                );
                let calls = Arc::new(AtomicUsize::new(0));
                let called = Arc::clone(&calls);
                let enabled = Arc::new(AtomicBool::new(true));
                let fail_enabled = Arc::clone(&enabled);
                let error = callback_failure(&failure, typed);
                let clock_now = JsString::from(now.as_str());
                let store = MultiAgentStore::new(MultiAgentStoreOptions {
                    now: Some(Arc::new(move || {
                        let count = called.fetch_add(1, Ordering::SeqCst) + 1;
                        if fail_enabled.load(Ordering::SeqCst) && count == throw_at {
                            Err(error.clone())
                        } else {
                            Ok(clock_now.clone())
                        }
                    })),
                });
                let record = || {
                    store.record_outbound_session_message(&RecordOutboundSessionMessageInput {
                        from_agent_id: "main".into(),
                        to_agent_id: "remote".into(),
                        body: "probe".into(),
                        thread_id: None,
                        file_refs: None,
                    })
                };
                let first = if operation == "restore" {
                    let db = native_dir.join("control.sqlite");
                    let setup = call(
                        &mut source,
                        json!({"op":"restoreSetup","dir":native_dir,"db":db,"now":now,"count":2}),
                    );
                    let mut manager =
                        SessionManager::open(&setup["path"].as_str().unwrap().into(), None, None).unwrap();
                    manager
                        .set_metadata_control_db_path(Some(&db.to_str().unwrap().into()), Default::default())
                        .unwrap();
                    captured(
                        store
                            .restore_from_session_manager(&manager)
                            .map(|()| JsValue::Null)
                            .map_err(CallbackError::from),
                    )
                } else {
                    captured(
                        record()
                            .map(|message| pi_js_compat::js_json::to_value(&message).unwrap())
                            .map_err(CallbackError::from),
                    )
                };
                let samples = calls.load(Ordering::SeqCst);
                enabled.store(false, Ordering::SeqCst);
                let probe = if operation == "restore" {
                    Value::Null
                } else {
                    serde_json::to_value(record().unwrap()).unwrap()
                };
                let messages = store.list_mailbox_messages().unwrap();
                assert_eq!(
                    JsValue::from(json!({"first":first,"samples":samples,"probe":probe,"messages":messages})),
                    JsValue::from(expected),
                    "{operation} throw{throw_at} typed{typed}"
                );
            }
        }
    }
}

#[test]
fn steering_publication_checks_creation_age_and_clock_before_mutating_state() {
    use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentMailboxMessage};
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    for (expired, throw_at) in [(false, 0), (true, 0), (false, 1)] {
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let expected = call(
            &mut source,
            json!({"op":"steering","now":now,"expired":expired,"typed":true,"throwAt":throw_at}),
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let called = Arc::clone(&calls);
        let enabled = Arc::new(AtomicBool::new(true));
        let fail_enabled = Arc::clone(&enabled);
        let clock_now = JsString::from(now.as_str());
        let store = MultiAgentStore::new(MultiAgentStoreOptions {
            now: Some(Arc::new(move || {
                let count = called.fetch_add(1, Ordering::SeqCst) + 1;
                if fail_enabled.load(Ordering::SeqCst) && count == throw_at {
                    Err(callback_failure(&Value::Null, true))
                } else {
                    Ok(clock_now.clone())
                }
            })),
        });
        let running:AgentSnapshot=from_value(&JsValue::from(json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}}))).unwrap();
        store.publish_lifecycle_coordinator_snapshot(&running).unwrap();
        let mut updated = running.clone();
        updated.lifecycle = AgentLifecycle::SteeringPending;
        updated.revision = 2;
        let message:AgentMailboxMessage=from_value(&JsValue::from(json!({"id":"steering","fromAgentId":"main","toAgentId":"child","kind":"steer","status":"pending","createdAt":if expired{"2020-01-01T00:00:00.000Z"}else{now.as_str()},"updatedAt":now,"body":"Steer 漢😀"}))).unwrap();
        let first = captured(
            store
                .publish_lifecycle_coordinator_steering(&updated, &message)
                .map(|()| JsValue::Null)
                .map_err(CallbackError::from),
        );
        let samples = calls.load(Ordering::SeqCst);
        enabled.store(false, Ordering::SeqCst);
        let agent = store.get_agent(&"child".into());
        let messages = store.list_mailbox_messages().unwrap();
        assert_eq!(
            JsValue::from(json!({"first":first,"samples":samples,"agent":agent,"messages":messages})),
            JsValue::from(expected)
        );
    }
}

#[test]
fn restore_samples_one_clock_for_zero_or_multiple_persisted_messages() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    for count in [None, Some(0), Some(2)] {
        let source_dir = root.path().join(format!("source-{count:?}"));
        let native_dir = root.path().join(format!("native-{count:?}"));
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&native_dir).unwrap();
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let expected = call(
            &mut source,
            json!({"op":"restore","dir":source_dir,"db":source_dir.join("control.sqlite"),"now":now,"count":count}),
        );
        let db = native_dir.join("control.sqlite");
        let setup = call(
            &mut source,
            json!({"op":"restoreSetup","dir":native_dir,"db":db,"now":now,"count":count}),
        );
        let path = setup["path"].as_str().unwrap();
        let mut manager = SessionManager::open(&path.into(), None, None).unwrap();
        manager
            .set_metadata_control_db_path(Some(&db.to_str().unwrap().into()), Default::default())
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let clock_calls = Arc::clone(&calls);
        let store = MultiAgentStore::new(MultiAgentStoreOptions {
            now: Some(Arc::new(move || {
                clock_calls.fetch_add(1, Ordering::SeqCst);
                Ok(now.clone().into())
            })),
        });
        store.restore_from_session_manager(&manager).unwrap();
        let samples = calls.load(Ordering::SeqCst);
        let messages = store.list_mailbox_messages().unwrap();
        assert_eq!(
            JsValue::from(json!({"first":{"ok":true,"value":null},"samples":samples,"probe":null,"messages":messages})),
            JsValue::from(expected),
            "{count:?}"
        );
    }
}

#[test]
fn slot_projection_uses_locale_order_and_conflict_uses_insertion_order() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    for ids in [["z", "ä", "A", "a"], ["ä", "a", "z", "A"]] {
        let expected = call(
            &mut source,
            json!({"op":"slots","ids":ids,"now":now,"dir":root.path(),"db":root.path().join("control.sqlite")}),
        );
        let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
        for id in ids.into_iter().chain(["target"]) {
            let mut agent = json!({"id":id,"parentId":"main","displayName":id,"agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
            if id != "target" {
                agent["slot"] = json!({"index":1,"pinned":true});
            }
            let agent: AgentSnapshot = from_value(&JsValue::from(agent)).unwrap();
            store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
        }
        let order = store
            .get_projection_snapshot()
            .unwrap()
            .slots
            .into_iter()
            .map(|slot| slot.agent_id)
            .collect::<Vec<_>>();
        let selection = store.select_agent_slot(1).unwrap().id;
        let AgentMetadataCommandResult::SlotConflict { occupant, .. } =
            store.pin_agent_slot(&"target".into(), 1, 1).unwrap()
        else {
            panic!("expected conflict")
        };
        assert_eq!(
            json!({"order":order,"selection":selection,"occupant":occupant.id}),
            expected
        );
    }
}

#[test]
fn empty_database_binding_does_not_create_a_store_persistence_target() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-store-callbacks-oracle.mts", "packages/coding-agent/src/core");
    for (index, (memory, empty_db, no_db)) in [
        (false, true, false),
        (false, false, false),
        (true, true, false),
        (true, false, false),
        (false, false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let directory = root.path().join(index.to_string());
        std::fs::create_dir(&directory).unwrap();
        let db = if empty_db {
            String::new()
        } else {
            directory.join("control.sqlite").to_str().unwrap().to_owned()
        };
        let expected = call(
            &mut source,
            json!({"op":"binding","dir":directory,"db":db,"memory":memory,"noDb":no_db}),
        );
        let mut manager = if memory {
            SessionManager::in_memory(Some(&directory.to_str().unwrap().into()), None).unwrap()
        } else {
            SessionManager::open(&expected["path"].as_str().unwrap().into(), None, None).unwrap()
        };
        let native_db = JsString::from(db.as_str());
        manager
            .set_metadata_control_db_path(if no_db { None } else { Some(&native_db) }, Default::default())
            .unwrap();
        let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
        store.set_persistence_session_manager(Some(&manager));
        let actual = store
            .get_persistence_target()
            .map(|target| json!({"controlDbPath":target.control_db_path,"sessionPath":target.session_path}))
            .unwrap_or(Value::Null);
        assert_eq!(
            actual, expected["persistence"],
            "memory{memory} empty{empty_db} none{no_db}"
        );
    }
}
