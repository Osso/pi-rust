//! Differential proof for the in-memory multi-agent store over operation scripts.

#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Arc;

use pi_coding_agent::core::multi_agent_store::{
    AgentSnapshot, ContactParentInput, MultiAgentStore, MultiAgentStoreOptions, RecordOutboundSessionMessageInput,
    SendMailboxMessageInput,
};
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};

fn store() -> MultiAgentStore {
    MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-21T00:00:00.000Z".into()))),
    })
}

fn agent(id: &str, parent: Option<&str>, lifecycle: &str, revision: i64) -> Value {
    json!({
        "id": id,
        "parentId": parent,
        "displayName": id,
        "agentType": "scout",
        "lifecycle": lifecycle,
        "revision": revision,
        "createdAt": "2026-06-21T00:00:00.000Z",
        "updatedAt": "2026-06-21T00:00:00.000Z",
        "cwd": "/repo",
        "permission": { "policy": "on-request", "narrowed": true },
    })
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, JsString> {
    js_json::to_value(&value).map_err(|error| JsString::from(error.to_string()))
}

fn parse_agent(value: &Value) -> Result<AgentSnapshot, JsString> {
    js_json::from_value(&value.clone()).map_err(|error| JsString::from(error.to_string()))
}

fn execute(store: &MultiAgentStore, op: &str, args: &[Value]) -> Result<Value, JsString> {
    match op {
        "publishLifecycleCoordinatorSnapshot" => {
            store
                .publish_lifecycle_coordinator_snapshot(&parse_agent(&args[0])?)
                .map_err(|e| e.message)?;
            Ok(Value::Null)
        }
        "publishTerminalOutboxSnapshot" => {
            store
                .publish_terminal_outbox_snapshot(&parse_agent(&args[0])?)
                .map_err(|e| e.message)?;
            Ok(Value::Null)
        }
        "expireMailboxMessages" => {
            store
                .expire_mailbox_messages(args[0].as_f64().ok_or("now")?)
                .map_err(|e| e.message)?;
            Ok(Value::Null)
        }
        "publishAgentModel" => {
            let model = js_json::from_value(&args[1].clone()).map_err(|e| JsString::from(e.to_string()))?;
            to_value(
                store
                    .publish_agent_model(args[0].as_string().ok_or("id")?, &model, None)
                    .map_err(|e| e.message)?,
            )
        }
        "publishAgentCurrentActivity" => {
            let activity = js_json::from_value(&args[1].clone()).map_err(|e| JsString::from(e.to_string()))?;
            let model = args
                .get(3)
                .filter(|v| !v.is_null())
                .map(|v| js_json::from_value(&v.clone()))
                .transpose()
                .map_err(|e| JsString::from(e.to_string()))?;
            to_value(
                store
                    .publish_agent_current_activity(args[0].as_string().ok_or("id")?, activity, None, model.as_ref())
                    .map_err(|e| e.message)?,
            )
        }
        "listAgents" => to_value(store.list_agents()),
        "listDescendants" => to_value(store.list_descendants(args[0].as_string().ok_or("parent")?)),
        "selectAgentView" => to_value(store.select_agent_view(args[0].as_string().ok_or("id")?)),
        "selectAgentViewWithStatus" => to_value(store.select_agent_view_with_status(args[0].as_string().ok_or("id")?)),
        "selectActiveAgentTargetWithStatus" => {
            to_value(store.select_active_agent_target_with_status(args[0].as_string().ok_or("id")?))
        }
        "selectAgentSlot" => to_value(store.select_agent_slot(args[0].as_f64().ok_or("slot")? as i64)),
        "getSelectedAgentId" => to_value(store.get_selected_agent_id()),
        "clearSelectedAgentView" => {
            store.clear_selected_agent_view();
            Ok(Value::Null)
        }
        "listActiveAgents" => to_value(store.list_active_agents()),
        "getActiveAgentCount" => to_value(store.get_active_agent_count()),
        "pinAgentSlot" => to_value(
            store
                .pin_agent_slot(
                    args[0].as_string().ok_or("id")?,
                    args[1].as_f64().ok_or("revision")? as i64,
                    args[2].as_f64().ok_or("slot")? as i64,
                )
                .map_err(|error| error.message.clone())?,
        ),
        "clearAgentSlot" => to_value(
            store
                .clear_agent_slot(
                    args[0].as_string().ok_or("id")?,
                    args[1].as_f64().ok_or("revision")? as i64,
                )
                .map_err(|error| error.message.clone())?,
        ),
        "updateAgentTranscript" => {
            let transcript =
                js_json::from_value(&args[1].clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                store
                    .update_agent_transcript(args[0].as_string().ok_or("id")?, &transcript)
                    .map_err(|error| error.message.clone())?,
            )
        }
        "contactParent" => {
            let input: ContactParentInput =
                js_json::from_value(&args[1].clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                store
                    .contact_parent(args[0].as_string().ok_or("id")?, &input)
                    .map_err(|error| error.message.clone())?,
            )
        }
        "sendMailboxMessage" => {
            let input: SendMailboxMessageInput =
                js_json::from_value(&args[2].clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                store
                    .send_mailbox_message(
                        args[0].as_string().ok_or("id")?,
                        args[1].as_f64().ok_or("revision")? as i64,
                        &input,
                    )
                    .map_err(|error| error.message.clone())?,
            )
        }
        "sendMainThreadMailboxMessage" => {
            let input: SendMailboxMessageInput =
                js_json::from_value(&args[0].clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                store
                    .send_main_thread_mailbox_message(&input)
                    .map_err(|error| error.message.clone())?,
            )
        }
        "recordOutboundSessionMessage" => {
            let input: RecordOutboundSessionMessageInput =
                js_json::from_value(&args[0].clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                store
                    .record_outbound_session_message(&input)
                    .map_err(|error| error.message.clone())?,
            )
        }
        "listMailboxMessages" => to_value(store.list_mailbox_messages().map_err(|e| e.message)?),
        "listPendingMailboxMessagesForAgent" => to_value(
            store
                .list_pending_mailbox_messages_for_agent(args[0].as_string().ok_or("id")?)
                .map_err(|e| e.message)?,
        ),
        "markMailboxMessageDelivered" => to_value(
            store
                .mark_mailbox_message_delivered(args[0].as_string().ok_or("id")?)
                .map_err(|e| e.message)?,
        ),
        "markMailboxMessageFailed" => to_value(
            store
                .mark_mailbox_message_failed(args[0].as_string().ok_or("id")?, args[1].as_string().ok_or("error")?)
                .map_err(|e| e.message)?,
        ),
        "consumeCompletionNotificationsForAgent" => to_value(
            store
                .consume_completion_notifications_for_agent(args[0].as_string().ok_or("id")?)
                .map_err(|e| e.message)?,
        ),
        "consumeFailureNotificationsForAgent" => to_value(
            store
                .consume_failure_notifications_for_agent(args[0].as_string().ok_or("id")?)
                .map_err(|e| e.message)?,
        ),
        "listPendingLifecycleNotificationsForAgent" => {
            let lifecycle = match args[1]
                .as_string()
                .ok_or("lifecycle")?
                .to_utf8()
                .expect("ASCII fixture lifecycle")
                .as_str()
            {
                "completed" => pi_coding_agent::core::multi_agent_store::LifecycleNotificationLifecycle::Completed,
                "failed" => pi_coding_agent::core::multi_agent_store::LifecycleNotificationLifecycle::Failed,
                "waiting_for_input" => {
                    pi_coding_agent::core::multi_agent_store::LifecycleNotificationLifecycle::WaitingForInput
                }
                other => return Err(pi_js_compat::js_format!("unknown lifecycle {}", other)),
            };
            to_value(
                store
                    .list_pending_lifecycle_notifications_for_agent(args[0].as_string().ok_or("id")?, lifecycle)
                    .map_err(|e| e.message)?,
            )
        }
        "getProjectionSnapshot" => to_value(store.get_projection_snapshot().map_err(|e| e.message)?),
        "getAgent" => to_value(store.get_agent(args[0].as_string().ok_or("id")?)),
        "allocateAgentIdForLifecycleCoordinator" => Ok(Value::String(
            store
                .allocate_agent_id_for_lifecycle_coordinator(
                    args[0]
                        .as_string()
                        .map(|value| value.to_utf8().expect("ASCII fixture agent prefix"))
                        .as_deref()
                        .unwrap_or("agent"),
                )
                .map_err(|e| e.message)?,
        )),
        "getRestoreGeneration" => to_value(store.get_restore_generation()),
        other => Err(format!("unsupported store operation {other}").into()),
    }
}

fn step(op: &str, args: Vec<Value>) -> Value {
    json!({ "op": op, "args": args })
}

fn script() -> Vec<Value> {
    let scout1 = agent("scout1", Some("root"), "running", 1);
    let scout2 = agent("scout2", Some("root"), "running", 1);
    let child = agent("child", Some("scout1"), "running", 1);
    let grandchild = agent("grandchild", Some("child"), "running", 1);
    let completed = agent("scout2", Some("root"), "completed", 2);
    vec![
        step("publishLifecycleCoordinatorSnapshot", vec![scout1.clone()]),
        step("publishLifecycleCoordinatorSnapshot", vec![scout2.clone()]),
        step("publishLifecycleCoordinatorSnapshot", vec![child.clone()]),
        step("publishLifecycleCoordinatorSnapshot", vec![grandchild.clone()]),
        step(
            "publishAgentModel",
            vec![
                json!("child"),
                json!({"providerId":"fixture","modelId":"new","thinkingLevel":"high"}),
            ],
        ),
        step(
            "publishAgentCurrentActivity",
            vec![
                json!("child"),
                json!({"phase":"tool","startedAt":"2026-06-21T00:00:00.000Z","toolCallId":"tool","toolName":"read","detail":"/fixture/a.rs"}),
                Value::Null,
                json!({"providerId":"fixture","modelId":"effective"}),
            ],
        ),
        step("listAgents", vec![]),
        step("listDescendants", vec![json!("scout1")]),
        step("selectAgentView", vec![json!("scout1")]),
        step("getSelectedAgentId", vec![]),
        step("selectActiveAgentTargetWithStatus", vec![json!("scout1")]),
        step("publishTerminalOutboxSnapshot", vec![completed]),
        step("selectActiveAgentTargetWithStatus", vec![json!("scout2")]),
        step("listActiveAgents", vec![]),
        step("getActiveAgentCount", vec![]),
        step("pinAgentSlot", vec![json!("scout1"), json!(1), json!(2)]),
        step("pinAgentSlot", vec![json!("child"), json!(1), json!(2)]),
        step("clearAgentSlot", vec![json!("scout1"), json!(2)]),
        step(
            "updateAgentTranscript",
            vec![
                json!("child"),
                json!({"sessionId": "child-session", "path": "/tmp/child.jsonl"}),
            ],
        ),
        step(
            "contactParent",
            vec![json!("child"), json!({"body": "need input", "threadId": "thread-1"})],
        ),
        step(
            "sendMailboxMessage",
            vec![
                json!("child"),
                json!(1),
                json!({"body": "hello parent", "toAgentId": "scout1"}),
            ],
        ),
        step("listPendingMailboxMessagesForAgent", vec![json!("scout1")]),
        step("markMailboxMessageDelivered", vec![json!("message_1")]),
        step(
            "recordOutboundSessionMessage",
            vec![json!({
                "fromAgentId": "main",
                "toAgentId": "remote",
                "body": "outbound",
                "threadId": "thread-2"
            })],
        ),
        step("getProjectionSnapshot", vec![]),
        step("allocateAgentIdForLifecycleCoordinator", vec![json!("bash")]),
        step("consumeCompletionNotificationsForAgent", vec![json!("scout2")]),
        step(
            "listPendingLifecycleNotificationsForAgent",
            vec![json!("scout2"), json!("completed")],
        ),
        step("listMailboxMessages", vec![]),
        step("getAgent", vec![json!("child")]),
        step(
            "publishLifecycleCoordinatorSnapshot",
            vec![agent("no-parent", Some(""), "running", 1)],
        ),
        step("contactParent", vec![json!("no-parent"), json!({"body":"no parent"})]),
    ]
}

#[test]
fn store_operation_scripts_match_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "multi-agent-store-oracle.ts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    let steps = script();
    let response = oracle.request(&json!({
        "id": 9101,
        "now": "2026-06-21T00:00:00.000Z",
        "steps": steps,
    }));
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response:?}");
    let oracle_outputs = response["output"].as_array().expect("output").clone();
    assert_eq!(oracle_outputs.len(), steps.len());

    let store = store();
    for (index, step) in steps.iter().enumerate() {
        let expected = &oracle_outputs[index];
        let actual = execute(
            &store,
            &step["op"]
                .as_string()
                .expect("op")
                .to_utf8()
                .expect("ASCII fixture operation name"),
            step["args"].as_array().expect("args"),
        );
        match (expected.get("ok"), actual) {
            (Some(Value::Bool(true)), Ok(value)) => {
                // Compare complete values here; storage wire fixtures separately check byte ordering.
                assert_eq!(expected["value"], value, "step {index} {:?} mismatch", step["op"]);
            }
            (Some(Value::Bool(false)), Err(error)) => {
                assert_eq!(
                    expected["error"]["message"].as_string(),
                    Some(&error),
                    "step {index} error mismatch"
                );
            }
            (expected_ok, actual) => {
                panic!("step {index} outcome mismatch: {expected_ok:?} vs {actual:?}");
            }
        }
    }
}
