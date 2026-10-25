use super::*;
use std::time::Duration;
mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

// Frozen runtime:327 and :373 forward the supplied context after teardown;
// relocation also mutates the outgoing manager observed by shutdown consumers.
#[tokio::test]
async fn switch_and_relocate_forward_trust_context_at_the_frozen_boundaries() {
    use pi_coding_agent::core::agent_session_runtime::SwitchSessionOptions;
    use pi_coding_agent::core::extensions::types::{ExtensionMode, NoOpUiContext, ProjectTrustContext};
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for operation in ["relocate-trust", "switch-trust"] {
        let harness = build_harness(false, false).await;
        let source_root = harness._temp.path().join("source-trust");
        std::fs::create_dir(&source_root).unwrap();
        let expected = source.request(&json!({"op":operation,"root":source_root}));
        assert_eq!(expected["context"], json!({"targetCwd":true,"mode":"tui","hasUI":true}));
        let cwd = harness._temp.path().join("trust-target");
        std::fs::create_dir(&cwd).unwrap();
        let cwd = JsString::from(cwd.to_str().unwrap());
        let db = harness.runtime.session().control_db_path().unwrap();
        let mut manager = SessionManager::create(
            &cwd,
            Some(&harness._temp.path().join("target-sessions").to_str().unwrap().into()),
            None,
        )
        .unwrap();
        manager
            .set_metadata_control_db_path(Some(&db), Default::default())
            .unwrap();
        manager.persist_for_recovery().unwrap();
        let path = manager.get_session_file().unwrap().clone();
        let order = Arc::new(Mutex::new(Vec::<&str>::new()));
        let captured = Arc::new(Mutex::new(None));
        let outgoing = Arc::new(AtomicBool::new(false));
        let (events, capture, target, relocated) = (order.clone(), captured.clone(), cwd.clone(), outgoing.clone());
        let factory: pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory =
            Arc::new(move |request| {
                events.lock().unwrap().push("factory");
                let context = request
                    .project_trust_context
                    .map(|context| json!({"targetCwd":context.cwd==target,"mode":context.mode,"hasUI":context.has_ui}));
                let mut value = json!({"context":context,"order":*events.lock().unwrap()});
                if operation == "relocate-trust" {
                    value["outgoingManagerRelocated"] = json!(relocated.load(Ordering::SeqCst));
                }
                *capture.lock().unwrap() = Some(value);
                Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture factory stop")) })
            });
        let runtime = AgentSessionRuntime::new(
            harness.runtime.session(),
            harness.runtime.services(),
            factory,
            vec![],
            None,
        );
        let (events, relocated, current, target) = (order.clone(), outgoing.clone(), runtime.session(), cwd.clone());
        runtime.set_before_session_invalidate(Some(Arc::new(move || {
            events.lock().unwrap().push("shutdown");
            relocated.store(current.session_manager().get_cwd() == &target, Ordering::SeqCst);
            Ok(())
        })));
        let context = ProjectTrustContext {
            cwd: cwd.clone(),
            mode: ExtensionMode::Tui,
            has_ui: true,
            ui: Arc::new(NoOpUiContext),
        };
        let result = if operation == "switch-trust" {
            let events = order.clone();
            runtime
                .switch_session(
                    &path,
                    SwitchSessionOptions {
                        project_trust_context_factory: Some(Arc::new(move |cwd| {
                            events.lock().unwrap().push("trust");
                            Ok(ProjectTrustContext {
                                cwd: cwd.clone(),
                                ..context.clone()
                            })
                        })),
                        ..Default::default()
                    },
                )
                .await
                .map(|_| ())
        } else {
            runtime.relocate(&cwd, Some(context)).await
        };
        assert_eq!(result.unwrap_err().message, "fixture factory stop");
        assert_eq!(*captured.lock().unwrap(), Some(expected), "{operation}");
    }
}

#[tokio::test]
async fn new_session_parent_uses_frozen_truthiness_before_factory() {
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for parent in [None, Some(JsString::from("")), Some(JsString::from("/parent"))] {
        let captured = Arc::new(Mutex::new(None));
        let observed = captured.clone();
        let harness = build_harness_with_runtime_observer(
            false,
            false,
            false,
            Some(Arc::new(move |request| {
                if request.session_start_event.as_ref().is_some_and(|event| {
                    event.reason == pi_coding_agent::core::extensions::types::SessionStartReason::New
                }) {
                    *observed.lock().unwrap() =
                        Some(json!({"parent":request.session_manager.get_header().unwrap().parent_session()}));
                }
            })),
        )
        .await;
        let root = harness._temp.path().join("source-parent");
        std::fs::create_dir(&root).unwrap();
        let mut request = json!({"op":"new-parent","root":root});
        if let Some(parent) = &parent {
            request["parentSession"] = json!(parent);
        }
        let expected = source.request(&request);
        harness
            .runtime
            .new_session(NewSessionOptions {
                parent_session: parent.clone(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(*captured.lock().unwrap(), Some(expected), "parent {parent:?}");
    }
}

#[tokio::test]
async fn in_process_restart_notice_uses_frozen_truthiness_and_utf16() {
    use pi_js_compat::js_json::{JsObject, JsValue};
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for notice in [
        None,
        Some(JsString::from("")),
        Some(JsString::from_utf16(vec![82, 55296, 33, 57343, 0])),
    ] {
        let harness = build_harness(false, false).await;
        let root = harness._temp.path().join("source-notice");
        std::fs::create_dir(&root).unwrap();
        let mut request = JsObject::from([
            ("op".into(), JsValue::String("restart-notice".into())),
            ("root".into(), JsValue::String(root.to_str().unwrap().into())),
        ]);
        if let Some(notice) = &notice {
            request.insert("notice".into(), JsValue::String(notice.clone()));
        }
        let expected = source.request(&JsValue::Object(request));
        harness.runtime.restart(notice.as_ref(), false).await.unwrap();
        let notices: Vec<_> = harness
            .runtime
            .session()
            .session_manager()
            .get_entries()
            .iter()
            .filter_map(|entry| {
                entry
                    .as_custom_message()
                    .filter(|entry| entry.custom_type == "self_restart")
                    .map(|entry| entry.content.clone())
            })
            .collect();
        let actual = JsValue::Object(JsObject::from([
            ("results".into(), JsValue::Array(vec![])),
            ("timestampsValid".into(), JsValue::Bool(true)),
            ("notices".into(), pi_js_compat::js_json::to_value(&notices).unwrap()),
        ]));
        assert_eq!(actual, expected, "notice {notice:?}");
    }
}

const DEFAULT_RESTART_PROOF: &str = "PI_RUNTIME_DEFAULT_RESTART_PROOF";

#[test]
fn actual_default_runtime_restart_child() {
    let Some(root) = std::env::var_os(DEFAULT_RESTART_PROOF) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    if std::env::var("PI_SELF_RESTART_REQUEST").as_deref() == Ok("1") {
        let proof: serde_json::Value = serde_json::from_str(&std::env::var("PI_SELF_RESTART_PROMPT").unwrap()).unwrap();
        let before: serde_json::Value =
            serde_json::from_slice(&std::fs::read(proof["before"].as_str().unwrap()).unwrap()).unwrap();
        let session = std::env::var("PI_SELF_RESTART_SESSION").unwrap();
        let value = json!({"pid":std::process::id(),"samePid":before["pid"]==json!(std::process::id()),"oldPidMatches":std::env::var("PI_SELF_RESTART_OLD_PID").unwrap()==before["pid"].as_u64().unwrap().to_string(),"cwdRetained":json!(std::env::current_dir().unwrap())==before["cwd"],"sessionExists":std::path::Path::new(&session).exists(),"marker":std::env::var("PI_SELF_RESTART_REQUEST").unwrap()});
        std::fs::write(proof["after"].as_str().unwrap(), value.to_string()).unwrap();
        return;
    }
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let harness = build_harness(false, false).await;
        let proof =
            json!({"proof":"runtime-default","before":root.join("before.json"),"after":root.join("after.json")});
        std::fs::write(
            root.join("before.json"),
            json!({"pid":std::process::id(),"cwd":std::env::current_dir().unwrap()}).to_string(),
        )
        .unwrap();
        harness
            .runtime
            .restart(Some(&proof.to_string().into()), true)
            .await
            .unwrap();
        panic!("default process restarter returned");
    });
}

// Actual source constructor default + actual native default: no injected
// process hooks. Both must exec in place rather than hand off to another PID.
#[test]
fn default_runtime_process_restart_execs_in_place_like_frozen() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    let native_root = temp.path().join("native");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&native_root).unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut source = oracle::spawn_oracle_process(
        &lock,
        "self-restart-default-oracle.mts",
        "packages/coding-agent/src",
        &[("PI_SELF_RESTART_REQUEST", "0".into())],
        Stdio::piped(),
    );
    writeln!(
        source.stdin.take().unwrap(),
        "{}",
        json!({"root":source_root,"operation":"runtime-default"})
    )
    .unwrap();
    let output = source.wait_with_output().unwrap();
    assert!(output.status.success(), "source: {output:?}");
    let mut expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source_root.join("after.json")).unwrap()).unwrap();
    assert_eq!(expected["samePid"], true, "source default must exec, not spawn");
    expected.as_object_mut().unwrap().remove("pid");
    let child = Command::new(std::env::current_exe().unwrap())
        .current_dir(&native_root)
        .args([
            "--exact",
            "runtime_restart::actual_default_runtime_restart_child",
            "--nocapture",
        ])
        .env(DEFAULT_RESTART_PROOF, &native_root)
        .env_remove("PI_SELF_RESTART_REQUEST")
        .env_remove("PI_SELF_RESTART_SESSION")
        .env_remove("PI_SELF_RESTART_PROMPT")
        .env_remove("PI_SELF_RESTART_OLD_PID")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let child_pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "native: {output:?}");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !native_root.join("after.json").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut actual: serde_json::Value =
        serde_json::from_slice(&std::fs::read(native_root.join("after.json")).unwrap()).unwrap();
    assert_eq!(
        actual["pid"], child_pid,
        "default restart must preserve the actual kernel PID"
    );
    actual.as_object_mut().unwrap().remove("pid");
    assert_eq!(actual, expected);
}

const RELOCATION_CHILD: &str = "PI_RUNTIME_RELOCATION_REPAIR_CHILD";

#[test]
fn relocation_repairs_deleted_physical_cwd_child() {
    let Some(root) = std::env::var_os(RELOCATION_CHILD) else {
        return;
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let harness = build_harness(false, false).await;
        let root = std::path::PathBuf::from(root);
        let target = root.join("target");
        std::fs::create_dir(&target).unwrap();
        let original = std::env::current_dir().unwrap();
        let capture = Arc::new(AtomicBool::new(false));
        let repaired = capture.clone();
        let expected = target.clone();
        let factory: pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory =
            Arc::new(|_| Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture factory stop")) }));
        let runtime = AgentSessionRuntime::new(
            harness.runtime.session(),
            harness.runtime.services(),
            factory,
            vec![],
            None,
        );
        runtime.set_before_session_invalidate(Some(Arc::new(move || {
            repaired.store(
                std::env::current_dir().is_ok_and(|cwd| cwd == expected),
                Ordering::SeqCst,
            );
            Ok(())
        })));
        runtime
            .prepare_tool_result_relocation(&target.to_str().unwrap().into())
            .await
            .unwrap();
        let physical = root.join("deleted");
        std::fs::create_dir(&physical).unwrap();
        std::env::set_current_dir(&physical).unwrap();
        std::fs::remove_dir(&physical).unwrap();
        let result = runtime.activate_tool_result_relocation().await;
        std::env::set_current_dir(&original).unwrap();
        assert_eq!(result.unwrap_err().message, "fixture factory stop");
        println!(
            "RELOCATION_REPAIR:{}",
            json!({"repairedBeforeInvalidate":capture.load(Ordering::SeqCst)})
        );
    });
}

// Frozen activateRelocation:359 repairs physical process cwd before teardown.
#[test]
fn relocation_repairs_deleted_physical_cwd_before_invalidation_like_frozen() {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().join("source");
    let native_root = root.path().join("native");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&native_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"relocate-repair","root":source_root}));
    assert_eq!(expected, json!({"repairedBeforeInvalidate":true}));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .current_dir(root.path())
        .args([
            "--exact",
            "runtime_restart::relocation_repairs_deleted_physical_cwd_child",
            "--nocapture",
        ])
        .env(RELOCATION_CHILD, &native_root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let actual: serde_json::Value = serde_json::from_str(
        text.lines()
            .find_map(|line| line.strip_prefix("RELOCATION_REPAIR:"))
            .expect("child observation"),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

// Frozen runtime:313-325 restores before open and binds DB before factory.
#[tokio::test]
async fn switch_restores_archive_and_binds_metadata_before_the_factory_like_frozen() {
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-switch");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"switch-archive","root":source_root}));
    assert_eq!(
        expected,
        json!({"restored":true,"metadataBound":true,"plainExists":true,"archiveExists":false})
    );
    let db = harness.runtime.session().control_db_path().unwrap();
    let mut target = SessionManager::create(
        &harness.runtime.cwd(),
        Some(&harness._temp.path().join("target").to_str().unwrap().into()),
        None,
    )
    .unwrap();
    target
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    target.persist_for_recovery().unwrap();
    let plain = target.get_session_file().unwrap().clone();
    let archive = pi_coding_agent::core::session_archive_storage::archive_persisted_session(&db, &plain).unwrap();
    let captured = Arc::new(Mutex::new(None));
    let capture = captured.clone();
    let target_archive = archive.clone();
    let factory: pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory = Arc::new(
        move |request| {
            *capture.lock().unwrap() = Some(
                json!({"restored":request.session_manager.get_session_file()==Some(&plain),"metadataBound":request.session_manager.get_metadata_control_db_path()==Some(&db),"plainExists":std::path::Path::new(&encode_node_utf8(&plain)).exists(),"archiveExists":std::path::Path::new(&encode_node_utf8(&target_archive)).exists()}),
            );
            Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture factory stop")) })
        },
    );
    let runtime = AgentSessionRuntime::new(
        harness.runtime.session(),
        harness.runtime.services(),
        factory,
        vec![],
        None,
    );
    let result = runtime.switch_session(&archive, Default::default()).await;
    assert_eq!(
        *captured.lock().unwrap(),
        Some(expected),
        "native switch error: {:?}",
        result.err().map(|error| error.message)
    );
}

#[tokio::test]
async fn switch_reads_the_manager_db_before_any_archive_side_effect_like_frozen() {
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-switch-no-db");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"switch-no-db","root":source_root}));
    assert_eq!(
        expected,
        json!({"error":"Error: Current session has no control database path","plainExists":false,"archiveExists":true})
    );
    let db = harness.runtime.session().control_db_path().unwrap();
    let mut target = SessionManager::create(
        &harness.runtime.cwd(),
        Some(&harness._temp.path().join("no-db-target").to_str().unwrap().into()),
        None,
    )
    .unwrap();
    target
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    target.persist_for_recovery().unwrap();
    let plain = target.get_session_file().unwrap().clone();
    let archive = pi_coding_agent::core::session_archive_storage::archive_persisted_session(&db, &plain).unwrap();
    harness
        .runtime
        .session()
        .session_manager()
        .set_metadata_control_db_path(None, Default::default())
        .unwrap();
    let error = harness
        .runtime
        .switch_session(&archive, Default::default())
        .await
        .unwrap_err();
    let actual = json!({"error":pi_coding_agent::utils::uncaught::format_uncaught(&error),"plainExists":std::path::Path::new(&encode_node_utf8(&plain)).exists(),"archiveExists":std::path::Path::new(&encode_node_utf8(&archive)).exists()});
    assert_eq!(actual, expected);
}

// Frozen runtime:509-517 transfers the outgoing metadata DB before factory.
#[tokio::test]
async fn new_session_transfers_metadata_db_before_the_factory_like_frozen() {
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-new");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"new-metadata","root":source_root}));
    assert_eq!(expected, json!({"factoryInheritedDb":true}));
    let db = harness.runtime.session().control_db_path().unwrap();
    let inherited = Arc::new(AtomicBool::new(false));
    let captured = inherited.clone();
    let factory: pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory =
        Arc::new(move |request| {
            captured.store(
                request.session_manager.get_metadata_control_db_path() == Some(&db),
                Ordering::SeqCst,
            );
            Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture factory stop")) })
        });
    let runtime = AgentSessionRuntime::new(
        harness.runtime.session(),
        harness.runtime.services(),
        factory,
        vec![],
        None,
    );
    assert_eq!(
        runtime.new_session(Default::default()).await.unwrap_err().message,
        "fixture factory stop"
    );
    assert_eq!(json!({"factoryInheritedDb":inherited.load(Ordering::SeqCst)}), expected);
}

// Frozen runtime:426-433 saves active settings before beforeProcessRestart.
#[tokio::test]
async fn process_restart_persists_model_and_thinking_before_the_hook_like_frozen() {
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-settings");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"persist-settings","root":source_root}));
    assert_eq!(
        expected,
        json!({"persisted":{"provider":"faux","model":"faux-1","thinking":"off"}})
    );
    let session = harness.runtime.session();
    session.session_manager().persist_for_recovery().unwrap();
    let db = session.control_db_path().unwrap();
    let path = session.session_file().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let persisted = captured.clone();
    harness.runtime.set_before_process_restart(Some(Arc::new(move || {
        let (db, path, persisted) = (db.clone(), path.clone(), persisted.clone());
        Box::pin(async move {
            let row = read_session_metadata(&db, &path).unwrap().unwrap();
            *persisted.lock().unwrap() =
                Some(json!({"provider":row.model_provider,"model":row.model_id,"thinking":row.thinking_level}));
            Ok(())
        })
    })));
    harness.runtime.set_process_restarter(Arc::new(|_| {
        Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture restarter stop")) })
    }));
    assert_eq!(
        harness.runtime.restart(None, true).await.unwrap_err().message,
        "fixture restarter stop"
    );
    assert_eq!(json!({"persisted":*captured.lock().unwrap()}), expected);
}

struct DetachOrderHandle {
    index: usize,
    order: Arc<Mutex<Vec<usize>>>,
}
impl pi_coding_agent::core::tool_detach_registry::ToolDetachHandle for DetachOrderHandle {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.order.lock().unwrap().push(self.index);
        Ok(self.index != 0)
    }
}

// Frozen runtime:438 and tool-detach-registry:35-37: every registered handle
// is tried in insertion order, even when one rejects or accepts detachment.
#[tokio::test]
async fn restart_detaches_all_running_handles_like_frozen_before_teardown() {
    let harness = build_harness(false, false).await;
    let registry = harness
        .runtime
        .session()
        .extension_runner()
        .create_context()
        .tool_detach_registry()
        .unwrap()
        .unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let subscriptions: Vec<_> = (0..3)
        .map(|index| {
            registry.register(Arc::new(DetachOrderHandle {
                index,
                order: order.clone(),
            }))
        })
        .collect();
    let source_root = harness._temp.path().join("source-detach");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"op":"detach","root":source_root}));
    assert_eq!(expected["detachOrder"], json!([0, 1, 2]));
    harness.runtime.restart(None, false).await.unwrap();
    assert_eq!(
        json!({"detachOrder":*order.lock().unwrap(),"results":[],"timestampsValid":true}),
        expected
    );
    drop(subscriptions);
}

fn seed_restart_jobs(db: &JsString, path: &JsString, jobs: &serde_json::Value) {
    pi_coding_agent::core::session_control_db::read_multi_agent_state(db, path).unwrap();
    let connection = rusqlite::Connection::open(encode_node_utf8(db)).unwrap();
    for (index, job) in jobs.as_array().unwrap().iter().enumerate() {
        connection
            .execute(
                "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,?,?,'fixture')",
                rusqlite::params![path, format!("job-{index}"), job.to_string()],
            )
            .unwrap();
    }
}

fn observe_restart_results(messages: &[AgentMessage], before: f64, after: f64) -> serde_json::Value {
    let results: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::ToolResult(result) => Some(serde_json::to_value(result).unwrap()),
            _ => None,
        })
        .collect();
    let timestamps_valid = results.iter().all(|message| {
        message["timestamp"]
            .as_f64()
            .is_some_and(|time| time >= before && time <= after)
    });
    let results: Vec<_> = results
        .into_iter()
        .map(|mut message| {
            message.as_object_mut().unwrap().remove("timestamp");
            message
        })
        .collect();
    json!({"results":results,"timestampsValid":timestamps_valid})
}

// Frozen runtime:438-466 + self-restart:166-203. Compare like boundaries:
// direct resolver retains DB; persisted runtime restart reopens without DB.
#[tokio::test]
async fn restart_resolves_pending_calls_like_the_actual_frozen_session_manager() {
    use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, ToolCall};
    let harness = build_harness(false, false).await;
    let mut pending = AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
    pending.stop_reason = StopReason::ToolUse;
    pending.content = [("r", "restart_self"), ("b", "bash"), ("x", "read")]
        .into_iter()
        .map(|(id, name)| AssistantContent::ToolCall(ToolCall::new(id, name, Default::default())))
        .collect();
    let jobs = json!([
        {"id":"not-detached","detached":false,"worker":{"toolCallId":"b"}},
        {"id":"job-first","detached":true,"worker":{"toolCallId":"b"}},
        {"id":"job-later","detached":true,"worker":{"toolCallId":"b"}},
        {"id":"unrelated","detached":true}
    ]);
    let source_root = harness._temp.path().join("source");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");

    let direct_dir = harness._temp.path().join("native-direct");
    let cwd = harness.runtime.cwd();
    let mut direct = SessionManager::create(&cwd, Some(&JsString::from(direct_dir.to_str().unwrap())), None).unwrap();
    let db = harness.runtime.session().control_db_path().unwrap();
    direct
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    seed_restart_jobs(&db, direct.get_session_file().unwrap(), &jobs);
    direct
        .append_message(AgentMessage::Assistant(Box::new(pending.clone())))
        .unwrap();
    let expected = source.request(&json!({"op":"resolve","root":source_root,"messages":[pending],"jobs":jobs}));
    assert_eq!(expected["results"][1]["details"]["backgroundJobId"], "job-first");
    let before = pi_js_compat::date::js_now_millis();
    pi_coding_agent::core::self_restart::resolve_restart_interrupted_tool_calls(&mut direct).unwrap();
    let after = pi_js_compat::date::js_now_millis();
    assert_eq!(
        observe_restart_results(&direct.build_session_context().messages, before, after),
        expected
    );

    let session = harness.runtime.session();
    seed_restart_jobs(&db, &session.session_file().unwrap(), &jobs);
    session
        .session_manager()
        .append_message(AgentMessage::Assistant(Box::new(pending.clone())))
        .unwrap();
    let expected = source.request(&json!({"op":"restart","root":source_root,"messages":[pending],"jobs":jobs}));
    assert_eq!(expected["results"].as_array().unwrap().len(), 3, "{expected}");
    assert_eq!(expected["results"][1]["isError"], true, "source loses DB across reopen");
    let before = pi_js_compat::date::js_now_millis();
    harness.runtime.restart(None, false).await.unwrap();
    let after = pi_js_compat::date::js_now_millis();
    let messages = harness
        .runtime
        .session()
        .session_manager()
        .build_session_context()
        .messages;
    assert_eq!(observe_restart_results(&messages, before, after), expected);
}
