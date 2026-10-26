//! Differential proof for `core/session-control-db.ts` over operation scripts.
//!
//! Both implementations run the same steps against their own temporary database.
//! Values are compared as source-ordered JSON; only generated ISO timestamps are
//! substituted in place.

use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_control_db::*;
use pi_coding_agent::core::session_health::SessionHealthRecord;
use pi_coding_agent::core::sqlite::create_sqlite_database;
use pi_js_compat::js_json;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use tempfile::TempDir;

fn temp_db() -> (TempDir, String) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    (directory, path)
}

fn looks_like_iso(value: &JsString) -> bool {
    let bytes = value.units();
    bytes.len() >= 20
        && bytes[4] == u16::from(b'-')
        && bytes[7] == u16::from(b'-')
        && bytes[10] == u16::from(b'T')
        && value.ends_with("Z")
}

fn mask_timestamps(value: &mut Value) {
    match value {
        Value::String(text) if looks_like_iso(text) => *text = "<timestamp>".into(),
        Value::Array(items) => items.iter_mut().for_each(mask_timestamps),
        Value::Object(object) => object.values_mut().for_each(mask_timestamps),
        _ => {}
    }
}

fn arg(args: &[Value], index: usize) -> Result<&Value, JsString> {
    args.get(index)
        .ok_or_else(|| format!("missing argument {index}").into())
}

fn arg_str(args: &[Value], index: usize) -> Result<&JsString, JsString> {
    arg(args, index)?
        .as_string()
        .ok_or_else(|| format!("argument {index} must be a string").into())
}

fn arg_i64(args: &[Value], index: usize) -> Result<i64, JsString> {
    arg(args, index)?
        .as_f64()
        .map(|value| value as i64)
        .ok_or_else(|| format!("argument {index} must be an integer").into())
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, JsString> {
    js_json::to_value(&value).map_err(|error| JsString::from(error.to_string()))
}

fn execute_call(function: &str, args: &[Value]) -> Result<Value, JsString> {
    match function {
        "writeSessionMetadata" => {
            let metadata: WritableSessionMetadata =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            write_session_metadata(arg_str(args, 0)?, &metadata).map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readSessionMetadata" => to_value(
            read_session_metadata(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?,
        ),
        "listSessionMetadata" => {
            to_value(list_session_metadata(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "listActiveSessionMetadata" => {
            to_value(list_active_session_metadata(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "listArchivedSessionMetadata" => {
            to_value(list_archived_session_metadata(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "writeSessionName" => {
            write_session_name(arg_str(args, 0)?, arg_str(args, 1)?, arg(args, 2)?.as_string())
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readSessionNameState" => to_value(
            read_session_name_state(arg_str(args, 0)?, arg_str(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "normalizeSessionName" => to_value(normalize_session_name(arg(args, 0)?.as_string())),
        "writeSessionModel" => {
            write_session_model(
                arg_str(args, 0)?,
                arg_str(args, 1)?,
                &arg_str(args, 2)?.to_utf8().expect("scalar fixture provider"),
                &arg_str(args, 3)?.to_utf8().expect("scalar fixture model ID"),
            )
            .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "writeSessionThinkingLevel" => {
            write_session_thinking_level(
                arg_str(args, 0)?,
                arg_str(args, 1)?,
                &arg_str(args, 2)?.to_utf8().expect("ASCII fixture thinking level"),
            )
            .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "writeSessionGoal" => {
            write_session_goal(arg_str(args, 0)?, arg_str(args, 1)?, arg(args, 2)?.as_string())
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readSessionGoal" => to_value(
            read_session_goal(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?,
        ),
        "recordPromptHistoryEntry" => {
            record_prompt_history_entry(arg_str(args, 0)?, arg_str(args, 1)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readPromptHistory" => to_value(
            read_prompt_history(arg_str(args, 0)?, arg_i64(args, 1)?).map_err(|error| error.message().to_owned())?,
        ),
        "readOrMigratePromptHistory" => {
            let entries: Vec<JsString> =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                read_or_migrate_prompt_history(arg_str(args, 0)?, &entries, arg_i64(args, 2)?)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "writeLastMessage" => {
            let message = arg(args, 1)?;
            let role = message
                .get("role")
                .and_then(Value::as_string)
                .ok_or_else(|| "writeLastMessage requires role".to_owned())?;
            let content = message
                .get("content")
                .and_then(Value::as_string)
                .ok_or_else(|| "writeLastMessage requires content".to_owned())?;
            write_last_message(
                arg_str(args, 0)?,
                &role.to_utf8().expect("ASCII fixture message role"),
                content,
            )
            .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readLastMessage" => {
            to_value(read_last_message(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "enqueueIncomingMessage" => to_value(
            enqueue_incoming_message(arg_str(args, 0)?, arg_str(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "claimLatestIncomingMessage" => {
            to_value(claim_latest_incoming_message(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "readIncomingMessageStatus" => to_value(
            read_incoming_message_status(arg_str(args, 0)?, arg_i64(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "completeIncomingMessage" => {
            complete_incoming_message(arg_str(args, 0)?, arg_i64(args, 1)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "failIncomingMessage" => {
            fail_incoming_message(arg_str(args, 0)?, arg_i64(args, 1)?, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "postSharedChannelMessage" => {
            let input: PostSharedChannelMessageInput =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                post_shared_channel_message(arg_str(args, 0)?, &input).map_err(|error| error.message().to_owned())?,
            )
        }
        "listSharedChannelMessagesAfter" => to_value(
            list_shared_channel_messages_after(
                arg_str(args, 0)?,
                arg_i64(args, 1)?,
                arg_i64(args, 2)?,
                args.get(3)
                    .and_then(Value::as_f64)
                    .map(|value| value as i64)
                    .unwrap_or(i64::MAX),
            )
            .map_err(|error| error.message().to_owned())?,
        ),
        "readSharedChannelTail" => {
            to_value(read_shared_channel_tail(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "initializeSharedChannelCursorAtTail" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                initialize_shared_channel_cursor_at_tail(arg_str(args, 0)?, &recipient)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "advanceSharedChannelCursor" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            advance_shared_channel_cursor(arg_str(args, 0)?, &recipient, arg_i64(args, 2)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readSharedChannelCursor" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                read_shared_channel_cursor(arg_str(args, 0)?, &recipient)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "postArchitectRequest" => {
            let input: PostArchitectRequestInput =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(post_architect_request(arg_str(args, 0)?, &input).map_err(|error| error.message().to_owned())?)
        }
        "listPendingArchitectRequests" => to_value(
            list_pending_architect_requests(arg_str(args, 0)?, arg_i64(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "claimPendingArchitectRequests" => to_value(
            claim_pending_architect_requests(arg_str(args, 0)?, arg_str(args, 1)?, arg_i64(args, 2)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "renewArchitectRequestClaims" => {
            let ids: Vec<i64> =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            renew_architect_request_claims(arg_str(args, 0)?, &ids, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "completeArchitectRequest" => {
            complete_architect_request(
                arg_str(args, 0)?,
                arg_i64(args, 1)?,
                arg_str(args, 2)?,
                args.get(3).and_then(Value::as_string),
            )
            .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "postSupervisorRequest" => {
            let input: PostSupervisorRequestInput =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(post_supervisor_request(arg_str(args, 0)?, &input).map_err(|error| error.message().to_owned())?)
        }
        "readSupervisorRequest" => to_value(
            read_supervisor_request(arg_str(args, 0)?, arg_i64(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "claimNextSupervisorRequest" => to_value(
            claim_next_supervisor_request(arg_str(args, 0)?, arg_str(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "completeSupervisorRequest" => {
            complete_supervisor_request(arg_str(args, 0)?, arg_i64(args, 1)?, arg_str(args, 2)?, arg(args, 3)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "hasPendingSupervisorApprovalRequest" => to_value(
            has_pending_supervisor_approval_request(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?,
        ),
        "recoverSupervisorRequests" => {
            recover_supervisor_requests(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "allocateMultiAgentCounter" => {
            let name = match arg_str(args, 2)?
                .to_utf8()
                .expect("ASCII fixture counter name")
                .as_str()
            {
                "agent" => MultiAgentCounterName::Agent,
                "message" => MultiAgentCounterName::Message,
                other => return Err(pi_js_compat::js_format!("unknown counter {}", other)),
            };
            to_value(
                allocate_multi_agent_counter(arg_str(args, 0)?, arg_str(args, 1)?, name)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "writeMultiAgentCounters" => {
            let counters: MultiAgentCounters =
                js_json::from_value(&arg(args, 2)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            write_multi_agent_counters(arg_str(args, 0)?, arg_str(args, 1)?, &counters)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "writeSessionHealth" => {
            let health: SessionHealthRecord =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            write_session_health(arg_str(args, 0)?, &health).map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readSessionHealth" => to_value(
            read_session_health(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?,
        ),
        "listSessionHealth" => {
            to_value(list_session_health(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "registerRuntimeMailboxListener" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            let pid = u32::try_from(arg_i64(args, 2)?).map_err(|error| JsString::from(error.to_string()))?;
            let session_path = arg(args, 3)?.as_string();
            let options = arg(args, 4)?;
            let options = RuntimeMailboxRegistrationOptions {
                runtime_instance_id: options
                    .get("runtimeInstanceId")
                    .and_then(Value::as_string)
                    .map(|value| value.to_utf8().expect("ASCII fixture runtime identity")),
                ..Default::default()
            };
            register_runtime_mailbox_listener(arg_str(args, 0)?, &recipient, pid, session_path, options)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readRuntimeMailboxListener" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                read_runtime_mailbox_listener(arg_str(args, 0)?, &recipient)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "listRuntimeMailboxListeners" => {
            to_value(list_runtime_mailbox_listeners(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "retireRuntimeMailboxListener" => {
            let recipient: RuntimeMailboxAddress =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                retire_runtime_mailbox_listener(
                    arg_str(args, 0)?,
                    &recipient,
                    u32::try_from(arg_i64(args, 2)?).map_err(|error| JsString::from(error.to_string()))?,
                )
                .map_err(|error| error.message().to_owned())?,
            )
        }
        "upsertMultiAgentMailboxMessage" => {
            upsert_multi_agent_mailbox_message(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?, arg(args, 3)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "getMultiAgentMailboxMessageStatus" => to_value(
            get_multi_agent_mailbox_message_status(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "markMultiAgentMailboxMessageDelivered" => to_value(
            mark_multi_agent_mailbox_message_delivered(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "markMultiAgentMailboxMessageFailed" => to_value(
            mark_multi_agent_mailbox_message_failed(
                arg_str(args, 0)?,
                arg_str(args, 1)?,
                arg_str(args, 2)?,
                arg_str(args, 3)?,
            )
            .map_err(|error| error.message().to_owned())?,
        ),
        "enqueueStoredRuntimeMailboxMessage" => {
            let input: EnqueueStoredRuntimeMailboxMessageInput =
                js_json::from_value(&arg(args, 1)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                enqueue_stored_runtime_mailbox_message(arg_str(args, 0)?, &input)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "listRuntimeMailboxMessages" => {
            to_value(list_runtime_mailbox_messages(arg_str(args, 0)?).map_err(|error| error.message().to_owned())?)
        }
        "readRuntimeMailboxMessage" => to_value(
            read_runtime_mailbox_message(arg_str(args, 0)?, arg_i64(args, 1)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "bootstrapMultiAgentAgent" => {
            bootstrap_multi_agent_agent(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?, arg(args, 3)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "readMultiAgentAgent" => to_value(
            read_multi_agent_agent(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?,
        ),
        "readMultiAgentAgents" => {
            let ids: Vec<JsString> =
                js_json::from_value(&arg(args, 2)?.clone()).map_err(|error| JsString::from(error.to_string()))?;
            to_value(
                read_multi_agent_agents(arg_str(args, 0)?, arg_str(args, 1)?, &ids)
                    .map_err(|error| error.message().to_owned())?,
            )
        }
        "readMultiAgentState" => to_value(
            read_multi_agent_state(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?,
        ),
        "listDetachedArtifactAgentsUpdatedAtOrBefore" => to_value(
            list_detached_artifact_agents_updated_at_or_before(
                arg_str(args, 0)?,
                &arg_str(args, 1)?.to_utf8().expect("ASCII fixture timestamp cutoff"),
            )
            .map_err(|error| error.message().to_owned())?,
        ),
        "archiveSession" => {
            archive_session(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "unarchiveSession" => {
            unarchive_session(arg_str(args, 0)?, arg_str(args, 1)?).map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        "relocateSessionControlData" => {
            relocate_session_control_data(arg_str(args, 0)?, arg_str(args, 1)?, arg_str(args, 2)?)
                .map_err(|error| error.message().to_owned())?;
            Ok(Value::Null)
        }
        other => Err(format!("unsupported function {other}").into()),
    }
}

fn execute_step(step: &Value) -> Result<Value, JsString> {
    if step.get("op").and_then(Value::as_string) != Some(&JsString::from("call")) {
        return Err("only call steps are supported".into());
    }
    let args = step
        .get("args")
        .and_then(Value::as_array)
        .ok_or_else(|| "call args must be an array".to_owned())?;
    let function = args
        .first()
        .and_then(Value::as_string)
        .ok_or_else(|| "call function name missing".to_owned())?;
    let call_args = args
        .get(1)
        .and_then(Value::as_array)
        .ok_or_else(|| "call arguments missing".to_owned())?;
    execute_call(&function.to_utf8().expect("ASCII fixture function name"), call_args)
}

fn metadata(session_path: &str, id: &str) -> Value {
    json!({
        "sessionPath": session_path,
        "id": id,
        "cwd": "/repo",
        "name": "Differential",
        "parentSessionPath": null,
        "archivedAt": null,
        "isArchived": false,
        "isSubagent": false,
        "subagentName": null,
        "modelProvider": null,
        "modelId": null,
        "thinkingLevel": null,
        "createdAt": "2026-06-21T00:00:00.000Z",
        "modifiedAt": "2026-06-21T00:00:00.000Z",
        "messageCount": 2,
        "firstMessage": "hello",
        "allMessagesText": "hello world",
        "indexMessageText": null,
    })
}

fn agent(id: &str, lifecycle: &str) -> Value {
    json!({
        "id": id,
        "parentId": "main",
        "displayName": "Scout",
        "agentType": "scout",
        "lifecycle": lifecycle,
        "revision": 1,
        "createdAt": "2026-06-21T00:00:00.000Z",
        "updatedAt": "2026-06-21T00:00:00.000Z",
        "cwd": "/repo",
        "permission": { "policy": "on-request", "narrowed": true },
    })
}

fn script(db: &str) -> Vec<Value> {
    let recipient = json!({"sessionId": "reader", "agentId": null});
    let sender = json!({"sessionId": "sender", "agentId": null});
    let stored = json!({
        "recipient": recipient,
        "sender": sender,
        "kind": "message",
        "storeRef": {"sessionPath": "/sessions/sender.jsonl", "messageId": "message_1"},
        "message": {
            "body": "stored",
            "createdAt": "2026-06-21T00:00:00.000Z",
            "fromAgentId": "main",
            "id": "message_1",
            "kind": "message",
            "status": "pending",
            "toAgentId": "main",
            "updatedAt": "2026-06-21T00:00:00.000Z",
        },
        "updatedAt": "2026-06-21T00:00:00.000Z",
    });
    let mut steps = vec![
        json!({"op":"call","args":["writeSessionMetadata",[db, metadata("/sessions/a.jsonl", "id-a")]]}),
        json!({"op":"call","args":["writeSessionMetadata",[db, metadata("/sessions/b.jsonl", "id-b")]]}),
        json!({"op":"call","args":["readSessionMetadata",[db, "/sessions/a.jsonl"]]}),
        json!({"op":"call","args":["listSessionMetadata",[db]]}),
        json!({"op":"call","args":["writeSessionModel",[db, "/sessions/a.jsonl", "anthropic", "claude"]]}),
        json!({"op":"call","args":["writeSessionThinkingLevel",[db, "/sessions/a.jsonl", "high"]]}),
        json!({"op":"call","args":["readSessionMetadata",[db, "/sessions/a.jsonl"]]}),
        json!({"op":"call","args":["writeSessionName",[db, "/sessions/a.jsonl", "  Named  "]]}),
        json!({"op":"call","args":["readSessionNameState",[db,"/sessions/a.jsonl"]]}),
        json!({"op":"call","args":["writeSessionGoal",[db, "/sessions/a.jsonl", "{\"objective\":\"ship\"}"]]}),
        json!({"op":"call","args":["readSessionGoal",[db, "/sessions/a.jsonl"]]}),
        json!({"op":"call","args":["archiveSession",[db, "/sessions/b.jsonl"]]}),
        json!({"op":"call","args":["listActiveSessionMetadata",[db]]}),
        json!({"op":"call","args":["listArchivedSessionMetadata",[db]]}),
        json!({"op":"call","args":["unarchiveSession",[db, "/sessions/b.jsonl"]]}),
        json!({"op":"call","args":["recordPromptHistoryEntry",[db, "  first  "]]}),
        json!({"op":"call","args":["recordPromptHistoryEntry",[db, "first"]]}),
        json!({"op":"call","args":["recordPromptHistoryEntry",[db, "second"]]}),
        json!({"op":"call","args":["readPromptHistory",[db, 10]]}),
        json!({"op":"call","args":["readOrMigratePromptHistory",[db, ["one","two","two","three"], 10]]}),
        json!({"op":"call","args":["writeLastMessage",[db, {"role":"assistant","content":"last"}]]}),
        json!({"op":"call","args":["readLastMessage",[db]]}),
        json!({"op":"call","args":["enqueueIncomingMessage",[db, "first"]]}),
        json!({"op":"call","args":["enqueueIncomingMessage",[db, "second"]]}),
        json!({"op":"call","args":["claimLatestIncomingMessage",[db]]}),
        json!({"op":"call","args":["readIncomingMessageStatus",[db, 1]]}),
        json!({"op":"call","args":["completeIncomingMessage",[db, 2]]}),
        json!({"op":"call","args":["readIncomingMessageStatus",[db, 2]]}),
        json!({"op":"call","args":["postSharedChannelMessage",[db, {"sender": {"sessionId":"one","agentId":null}, "body":"  hello  "}]]}),
        json!({"op":"call","args":["postSharedChannelMessage",[db, {"sender": {"sessionId":"two","agentId":"agent"}, "body":"world"}]]}),
        json!({"op":"call","args":["listSharedChannelMessagesAfter",[db, 0, 20]]}),
        json!({"op":"call","args":["readSharedChannelTail",[db]]}),
        json!({"op":"call","args":["initializeSharedChannelCursorAtTail",[db, recipient]]}),
        json!({"op":"call","args":["advanceSharedChannelCursor",[db, recipient, 1]]}),
        json!({"op":"call","args":["readSharedChannelCursor",[db, recipient]]}),
        json!({"op":"call","args":["postArchitectRequest",[db, {"senderSessionId":"sender","projectCwd":"/repo","body":"  question  "}]]}),
        json!({"op":"call","args":["listPendingArchitectRequests",[db, 20]]}),
        json!({"op":"call","args":["claimPendingArchitectRequests",[db, "token", 20]]}),
        json!({"op":"call","args":["renewArchitectRequestClaims",[db, [1], "token"]]}),
        json!({"op":"call","args":["completeArchitectRequest",[db, 1, "token", "sender"]]}),
        json!({"op":"call","args":["postSupervisorRequest",[db, {"senderSessionId":"sender","projectId":"project","kind":"approval_review","payload":{"command":"ls"},"deadlineAt":"2099-01-01T00:00:00.000Z"}]]}),
        json!({"op":"call","args":["postSupervisorRequest",[db, {"senderSessionId":"sender","projectId":"project","kind":"supervisor_advisory","payload":{},"deadlineAt":"2099-01-01T00:00:00.000Z"}]]}),
        json!({"op":"call","args":["hasPendingSupervisorApprovalRequest",[db]]}),
        json!({"op":"call","args":["claimNextSupervisorRequest",[db, "claim"]]}),
        json!({"op":"call","args":["completeSupervisorRequest",[db, 1, "claim", {"kind":"approve","reason":"ok"}]]}),
        json!({"op":"call","args":["readSupervisorRequest",[db, 1]]}),
        json!({"op":"call","args":["allocateMultiAgentCounter",[db, "/sessions/c.jsonl", "agent"]]}),
        json!({"op":"call","args":["allocateMultiAgentCounter",[db, "/sessions/c.jsonl", "agent"]]}),
        json!({"op":"call","args":["allocateMultiAgentCounter",[db, "/sessions/c.jsonl", "message"]]}),
        json!({"op":"call","args":["writeMultiAgentCounters",[db, "/sessions/d.jsonl", {"nextAgentNumber":7,"nextMessageNumber":9}]]}),
        json!({"op":"call","args":["allocateMultiAgentCounter",[db, "/sessions/d.jsonl", "agent"]]}),
        json!({"op":"call","args":["writeSessionHealth",[db, {"sessionId":"health","agentGeneration":3,"pid":42,"lastActiveAt":"2026-06-21T00:00:00.000Z","lastCheckedAt":"2026-06-21T00:00:01.000Z","checkStatus":"timeout","checkedGeneration":2,"checkLatencyMs":12,"updatedAt":"2026-06-21T00:00:02.000Z"}]]}),
        json!({"op":"call","args":["readSessionHealth",[db, "health"]]}),
        json!({"op":"call","args":["listSessionHealth",[db]]}),
        json!({"op":"call","args":["registerRuntimeMailboxListener",[db, recipient, 4242, "/sessions/reader.jsonl", {"runtimeInstanceId":"{\"pid\":4242,\"startTimeTicks\":7}"}]]}),
        json!({"op":"call","args":["readRuntimeMailboxListener",[db, recipient]]}),
        json!({"op":"call","args":["listRuntimeMailboxListeners",[db]]}),
        json!({"op":"call","args":["retireRuntimeMailboxListener",[db, recipient, 4242]]}),
        json!({"op":"call","args":["upsertMultiAgentMailboxMessage",[db, "/sessions/sender.jsonl", "message_1", {"body":"stored","fromAgentId":"main","id":"message_1","kind":"message","status":"pending","toAgentId":"main"}]]}),
        json!({"op":"call","args":["getMultiAgentMailboxMessageStatus",[db, "/sessions/sender.jsonl", "message_1"]]}),
        json!({"op":"call","args":["markMultiAgentMailboxMessageDelivered",[db, "/sessions/sender.jsonl", "message_1"]]}),
        json!({"op":"call","args":["getMultiAgentMailboxMessageStatus",[db, "/sessions/sender.jsonl", "message_1"]]}),
        json!({"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db, stored]]}),
        json!({"op":"call","args":["listRuntimeMailboxMessages",[db]]}),
        json!({"op":"call","args":["readRuntimeMailboxMessage",[db, 1]]}),
        json!({"op":"call","args":["bootstrapMultiAgentAgent",[db, "/sessions/agents.jsonl", "agent_1", agent("agent_1", "running")]]}),
        json!({"op":"call","args":["bootstrapMultiAgentAgent",[db, "/sessions/agents.jsonl", "agent_2", agent("agent_2", "completed")]]}),
        json!({"op":"call","args":["readMultiAgentAgent",[db, "/sessions/agents.jsonl", "agent_1"]]}),
        json!({"op":"call","args":["readMultiAgentAgents",[db, "/sessions/agents.jsonl", ["agent_1","agent_2","missing"]]]}),
        json!({"op":"call","args":["readMultiAgentState",[db, "/sessions/agents.jsonl"]]}),
        json!({"op":"call","args":["relocateSessionControlData",[db, "/sessions/a.jsonl", "/sessions/moved.jsonl"]]}),
        json!({"op":"call","args":["readSessionMetadata",[db, "/sessions/moved.jsonl"]]}),
        json!({"op":"call","args":["readSessionNameState",[db,"/sessions/moved.jsonl"]]}),
    ];
    steps.push(json!({"op":"call","args":["postSharedChannelMessage",[db, {"sender": {"sessionId":"x","agentId":null}, "body":"   "}]]}));
    steps
}

#[test]
fn operation_scripts_match_the_pinned_typescript_module() {
    let (_directory, rust_db) = temp_db();
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let (_oracle_directory, oracle_db) = temp_db();
    let request = json!({
        "id": 9001,
        "steps": script(&oracle_db),
    });
    let response = oracle.request(&request);
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response:?}");
    let oracle_outputs = response["output"].as_array().expect("oracle output").clone();
    let rust_steps = script(&rust_db);
    assert_eq!(oracle_outputs.len(), rust_steps.len());

    for (index, step) in rust_steps.iter().enumerate() {
        let expected = &oracle_outputs[index];
        let actual = execute_step(step);
        match (expected.get("ok"), actual) {
            (Some(Value::Bool(true)), Ok(value)) => {
                let mut expected_value = expected["value"].clone();
                let mut actual_value = value;
                mask_timestamps(&mut expected_value);
                mask_timestamps(&mut actual_value);
                assert_eq!(
                    js_json::stringify_lossless(&expected_value),
                    js_json::stringify_lossless(&actual_value),
                    "step {index} {:?} mismatch",
                    step
                );
            }
            (Some(Value::Bool(false)), Err(error)) => {
                assert_eq!(
                    expected["error"]["message"].as_string(),
                    Some(&error),
                    "step {index} error mismatch"
                );
            }
            (expected_ok, actual) => {
                panic!("step {index} outcome mismatch: expected {expected_ok:?}, got {actual:?}");
            }
        }
    }
}

#[test]
fn unsupported_future_schema_errors_match() {
    let (_directory, db) = temp_db();
    let connection = create_sqlite_database(&db).expect("open");
    connection.execute_batch("PRAGMA user_version = 99").expect("version");
    drop(connection);
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let response = oracle.request(&json!({
        "id": 9002,
        "steps": [{"op":"call","args":["readSessionHealth",[db.clone(), "missing"]]}],
    }));
    let oracle_message = response["output"][0]["error"]["message"]
        .as_string()
        .unwrap()
        .to_owned();
    let error = read_session_health(&db, &JsString::from("missing")).expect_err("future schema");
    assert_eq!(*error.message(), oracle_message);
}

#[test]
fn empty_channel_body_error_matches() {
    let (_directory, db) = temp_db();
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let response = oracle.request(&json!({
        "id": 9003,
        "steps": [{"op":"call","args":["postSharedChannelMessage",[db.clone(), {"sender": {"sessionId":"x","agentId":null}, "body":"   "}]]}],
    }));
    let oracle_message = response["output"][0]["error"]["message"]
        .as_string()
        .unwrap()
        .to_owned();
    let error = post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: RuntimeMailboxAddress {
                session_id: "x".into(),
                agent_id: None,
            },
            body: "   ".into(),
        },
    )
    .expect_err("empty");
    assert_eq!(*error.message(), oracle_message);
}
