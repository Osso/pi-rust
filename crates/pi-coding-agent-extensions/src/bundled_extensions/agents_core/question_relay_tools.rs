//! Whole frozen `agents-core/src/question-relay-tools.ts` module.

use super::mailbox_runtime::{ContactParentParams, contact_parent, current_message_sender_id};
use super::registration::tool_definition;
use super::tool_results::tool_result;
use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError};
use crate::core::multi_agent_store::{MultiAgentStore, is_active_lifecycle};
use crate::core::session_control_db::{
    EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessage, RuntimeMailboxMessageKind,
    RuntimeMailboxStoreRef, enqueue_stored_runtime_mailbox_message, take_runtime_mailbox_messages_for_delivery,
};
use crate::core::tools::ask_questions::{
    self, AskQuestionsParams, Question, parse_params, question_result, validate_questions,
};
use pi_agent_core::AgentToolResult;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::{
    js_json as raw_json,
    js_json::{self, JsString, JsValue as Value},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::{self, JsString};

    #[test]
    fn question_and_answer_rendering_retain_unpaired_units() {
        let params = js_json::parse_lossless(r#"{"blocking":true,"questions":[{"question":"Q\ud800","header":"\udc00","options":[{"label":"A\ud800","description":"D\udc00"}]}]}"#).unwrap();
        let parsed = parse_params(&params).unwrap();
        let rendered = format_question(&parsed.questions[0], 0);
        let expected = JsString::from_utf16(vec![0xdc00]);
        assert!(rendered.contains(expected));
        assert!(rendered.contains(JsString::from_utf16(vec![0x51, 0xd800])));
        let answers = js_json::parse_lossless(r#"{"Q\ud800":"A\udc00"}"#).unwrap();
        let formatted = format_answer_payload(Some(&answers)).unwrap();
        assert_eq!(
            formatted,
            JsString::from("- Q") + JsString::from_utf16(vec![0xd800]) + " → A" + JsString::from_utf16(vec![0xdc00])
        );
    }
}

#[cfg(test)]
mod delivery_lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsValue;

    #[test]
    fn delivered_answers_keep_units_and_cancelled_omission_vs_null() {
        let params = js_json::parse_lossless(r#"{"blocking":true,"questions":[],"metadata":"\ud800"}"#).unwrap();
        let answers = js_json::parse_lossless(r#"{"Q\ud800":"A\udc00"}"#).unwrap();
        let inbox = Inbox {
            agent_id: "child".into(),
            db: "db".into(),
            session_id: "session".into(),
        };
        for cancelled in [None, Some(JsValue::Null), Some(JsValue::Bool(false))] {
            let mut payload = raw_json!({"type":"ask_questions_answer","answers":answers});
            if let Some(cancelled) = &cancelled {
                payload["cancelled"] = cancelled.clone();
            }
            let mut delivered = RuntimeMailboxMessage {
                id: 7,
                recipient: RuntimeMailboxAddress {
                    agent_id: Some("child".into()),
                    session_id: "session".into(),
                },
                sender: RuntimeMailboxAddress {
                    agent_id: None,
                    session_id: "parent".into(),
                },
                sender_host: None,
                kind: RuntimeMailboxMessageKind::Reply,
                body: js_json::stringify_lossless(&payload).into(),
                file_refs: None,
                target_checkpoint: None,
                store_ref: RuntimeMailboxStoreRef {
                    message_id: "answer".into(),
                    session_path: "path".into(),
                },
                status: crate::core::session_control_db::RuntimeMailboxMessageStatus::Claimed,
                created_at: "now".into(),
                updated_at: "now".into(),
                claimed_at: None,
                error: None,
            };
            let result = delivered_answer_result(&params, &inbox, &delivered).unwrap();
            let details = result.details.unwrap();
            assert_eq!(details["answers"], answers);
            assert_eq!(details.get("cancelled"), cancelled.as_ref());
            assert_eq!(details["metadata"], params["metadata"]);
            let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
                panic!("text result")
            };
            assert!(text.text.contains(JsString::from_utf16(vec![0xd800])));
            assert!(text.text.contains(JsString::from_utf16(vec![0xdc00])));

            delivered.body = JsString::from_utf16(vec![0xd800]);
            let error = delivered_answer_result(&params, &inbox, &delivered).unwrap_err();
            assert!(error.message.contains(JsString::from_utf16(vec![0xd800])));
        }
    }
}

fn handler(error: impl std::fmt::Display) -> HandlerError {
    HandlerError::new(error.to_string())
}
fn answer_message_id(agent_id: impl Into<JsString>, request_id: impl Into<JsString>) -> JsString {
    JsString::from("ask_questions:") + agent_id.into() + ":" + request_id.into()
}
fn format_question(question: &Question, index: usize) -> JsString {
    let header = question
        .header
        .as_ref()
        .filter(|header| !header.is_empty())
        .map(|header| JsString::from("[") + header + "] ")
        .unwrap_or_default();
    let mut lines = vec![JsString::from(format!("{}. ", index + 1)) + header + &question.question];
    let Some(options) = &question.options else {
        lines.push("   Free-text answer.".into());
        return JsString::join(lines, "\n");
    };
    let choice = if question.multi_select == Some(true) {
        "Choose one or more (comma-separated)"
    } else {
        "Choose one"
    };
    lines.push(format!("   {choice}, or give another answer:").into());
    for option in options {
        let description = option
            .description
            .as_ref()
            .filter(|description| !description.is_empty())
            .map(|description| JsString::from(" — ") + description)
            .unwrap_or_default();
        lines.push(JsString::from("   - ") + &option.label + description);
    }
    JsString::join(lines, "\n")
}
fn format_parent_request(agent_id: &JsString, request_id: &JsString, params: &AskQuestionsParams) -> JsString {
    let waiting = if params.blocking {
        "The child is blocked until you answer."
    } else {
        "The child continues meanwhile."
    };
    let mut lines = vec![
        js_format!("Child agent {} asks (requestId: {}). {}", agent_id, request_id, waiting),
        js_format!(
            "Reply with answer_question using agentId \"{}\" and requestId \"{}\", answers keyed by exact question text. Ask the user with ask_questions first when you cannot answer.",
            agent_id,
            request_id
        ),
        JsString::default(),
    ];
    lines.extend(
        params
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| format_question(question, index)),
    );
    JsString::join(lines, "\n")
}
struct Inbox {
    agent_id: JsString,
    db: JsString,
    session_id: JsString,
}
fn send_questions_to_parent(
    store: &Arc<MultiAgentStore>,
    request_id: &JsString,
    params: &AskQuestionsParams,
    ctx: &ExtensionContext,
) -> Result<Inbox, HandlerError> {
    let (Some(agent_id), Some(db)) = (ctx.multi_agent_agent_id()?, ctx.control_db_path()?) else {
        return Err(handler(
            "ask_questions in a child agent requires its agent identity and control database",
        ));
    };
    if agent_id.is_empty() || db.is_empty() {
        return Err(handler(
            "ask_questions in a child agent requires its agent identity and control database",
        ));
    }
    let sent = contact_parent(
        store,
        &ContactParentParams {
            message: format_parent_request(&agent_id, request_id, params),
            thread_id: Some(answer_message_id(&agent_id, request_id)),
            file_refs: None,
        },
        Some(ctx),
    )?;
    let message = &sent.details.as_ref().unwrap_or(&Value::Null)["message"];
    if message["id"].as_string().is_none_or(JsString::is_empty) || message["status"] == Value::String("failed".into()) {
        let reason = sent
            .content
            .iter()
            .filter_map(|part| match part {
                pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        return Err(HandlerError::new(
            JsString::from("Could not send questions to parent: ") + JsString::join(reason, ""),
        ));
    }
    let session = ctx.session_manager()?;
    let session_id = session.get_session_id().clone();
    Ok(Inbox {
        agent_id,
        db,
        session_id,
    })
}
async fn wait_for_answer(
    inbox: &Inbox,
    request_id: &JsString,
    signal: Option<AbortSignal>,
) -> Result<Option<RuntimeMailboxMessage>, HandlerError> {
    let message_id = answer_message_id(&inbox.agent_id, request_id);
    let recipient = RuntimeMailboxAddress {
        agent_id: Some(inbox.agent_id.clone()),
        session_id: inbox.session_id.clone(),
    };
    loop {
        if signal.as_ref().is_some_and(AbortSignal::aborted) {
            return Ok(None);
        }
        let mut messages = take_runtime_mailbox_messages_for_delivery(
            &inbox.db,
            &recipient,
            |message| Ok(message.store_ref.message_id == message_id),
            1,
        )
        .map_err(handler)?;
        if let Some(message) = messages.pop() {
            return Ok(Some(message));
        }
        let delay = tokio::time::sleep(std::time::Duration::from_millis(500));
        if let Some(signal) = &signal {
            tokio::select! { _ = delay => {}, _ = signal.wait() => return Ok(None) }
        } else {
            delay.await;
        }
    }
}
async fn ask_parent(
    store: Arc<MultiAgentStore>,
    request_id: JsString,
    params: Value,
    signal: Option<AbortSignal>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let parsed = parse_params(&params).map_err(|error| HandlerError::new(error.message))?;
    validate_questions(&parsed.questions).map_err(|error| HandlerError::new(error.message))?;
    let inbox = send_questions_to_parent(&store, &request_id, &parsed, &ctx)?;
    if !parsed.blocking {
        let mut result = question_result(
            &params,
            js_format!(
                "Questions sent to parent (request {}). Continue independent work; the answer will arrive in your inbox.",
                request_id
            ),
            raw_json!({}),
            false,
        );
        let details = result.details.as_mut().unwrap();
        details["queued"] = Value::Bool(true);
        details["requestId"] = Value::String(request_id);
        return Ok(result);
    }
    let Some(delivered) = wait_for_answer(&inbox, &request_id, signal).await? else {
        return Ok(question_result(&params, "Questions cancelled", raw_json!({}), true));
    };
    delivered_answer_result(&params, &inbox, &delivered)
}

fn delivered_answer_result(
    params: &Value,
    inbox: &Inbox,
    delivered: &RuntimeMailboxMessage,
) -> Result<AgentToolResult, HandlerError> {
    use pi_js_compat::js_json::{self, JsString, JsValue};
    let parsed = js_json::parse_lossless(&delivered.body).map_err(|error| HandlerError::new(error.into_message()))?;
    let JsValue::Object(answer) = parsed else {
        return Err(HandlerError::new(
            JsString::from("Malformed question answer ") + &delivered.store_ref.message_id,
        ));
    };
    if answer.get(&JsString::from("type")) != Some(&JsValue::String(JsString::from("ask_questions_answer"))) {
        return Err(HandlerError::new(
            JsString::from("Malformed question answer ") + &delivered.store_ref.message_id,
        ));
    }
    let answers = answer.get(&JsString::from("answers"));
    let text = format_answer_payload(answers)?;
    let cancelled = answer.get(&JsString::from("cancelled"));
    let outcome = if cancelled.is_some_and(js_json::is_truthy) {
        "cancelled"
    } else {
        "answered"
    };
    let mut result = question_result(
        params,
        JsString::from(format!("Parent {outcome} questions:\n")) + text,
        answers.cloned().unwrap_or(JsValue::Null),
        false,
    );
    let details = result.details.as_mut().unwrap();
    if let Some(cancelled) = cancelled {
        details["cancelled"] = cancelled.clone();
    } else {
        details
            .as_object_mut()
            .unwrap()
            .shift_remove(&JsString::from("cancelled"));
    }
    details["runtimeMailboxDelivery"] =
        raw_json!({"controlDbPath":inbox.db,"messages":[{"id":delivered.id,"storeRef":delivered.store_ref}]});
    Ok(result)
}

fn format_answer_payload(answers: Option<&pi_js_compat::js_json::JsValue>) -> Result<JsString, HandlerError> {
    use pi_js_compat::js_json::{self, JsString, JsValue};
    let entries = match answers {
        None | Some(JsValue::Null) => return Err(handler("Cannot convert undefined or null to object")),
        Some(JsValue::Object(object)) => js_json::object_entries_lossless(object)
            .into_iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>(),
        Some(JsValue::Array(values)) => values
            .iter()
            .enumerate()
            .map(|(index, value)| (JsString::from(index.to_string().as_str()), value.clone()))
            .collect(),
        Some(JsValue::String(text)) => text
            .units()
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                (
                    JsString::from(index.to_string().as_str()),
                    JsValue::String(JsString::from_utf16(vec![*unit])),
                )
            })
            .collect(),
        Some(JsValue::Bool(_) | JsValue::Number(_)) => Vec::new(),
    };
    entries
        .into_iter()
        .map(|(question, answer)| {
            let value = if js_json::is_truthy(&answer) {
                js_json::to_js_string(&answer).map_err(handler)?
            } else {
                JsString::from("(no selection)")
            };
            Ok(JsString::from("- ") + question + " → " + value)
        })
        .collect::<Result<Vec<_>, HandlerError>>()
        .map(|lines| JsString::join(lines, "\n"))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnswerQuestionParams {
    agent_id: JsString,
    request_id: JsString,
    answers: Value,
    cancelled: Option<bool>,
}
fn answer_question(
    store: &MultiAgentStore,
    params: AnswerQuestionParams,
    ctx: &ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let caller_id = current_message_sender_id(Some(ctx))?;
    let target = store.get_agent(&params.agent_id);
    let parent_id = target
        .as_ref()
        .and_then(|target| target.parent_id.clone())
        .unwrap_or_else(|| "main".into());
    if caller_id.is_none() || target.is_none() || Some(&parent_id) != caller_id.as_ref() {
        return Err(HandlerError::new(
            JsString::from("Could not answer ") + &params.agent_id + ": not a direct child of this agent",
        ));
    }
    let target = target.unwrap();
    if !is_active_lifecycle(target.lifecycle) {
        return Err(HandlerError::new(
            JsString::from("Could not answer ")
                + &params.agent_id
                + ": inactive ("
                + js_json::to_js_string(&js_json::to_value(&target.lifecycle).map_err(handler)?)
                    .map_err(HandlerError::new)?
                + ")",
        ));
    }
    let persistence = store.get_persistence_target();
    let session_id = target
        .transcript
        .as_ref()
        .map(|transcript| transcript.session_id.clone());
    let db = ctx.control_db_path()?;
    let (Some(persistence), Some(session_id)) = (persistence, session_id) else {
        return Err(HandlerError::new(
            JsString::from("Could not answer ") + &params.agent_id + ": runtime mailbox is unavailable",
        ));
    };
    if Some(&persistence.control_db_path) != db.as_ref() {
        return Err(HandlerError::new(
            JsString::from("Could not answer ") + &params.agent_id + ": runtime mailbox is unavailable",
        ));
    }
    let answer = raw_json!({"type":"ask_questions_answer","requestId":params.request_id,"answers":params.answers,"cancelled":params.cancelled == Some(true)});
    let message_id = answer_message_id(&params.agent_id, &params.request_id);
    let session = ctx.session_manager()?;
    let sender_session_id = session.get_session_id().to_owned();
    enqueue_stored_runtime_mailbox_message(&persistence.control_db_path, &EnqueueStoredRuntimeMailboxMessageInput {
        recipient_host:None, recipient:RuntimeMailboxAddress { agent_id:Some(params.agent_id.clone()), session_id }, sender:RuntimeMailboxAddress { agent_id:ctx.multi_agent_agent_id()?, session_id:sender_session_id }, kind:RuntimeMailboxMessageKind::Reply,
        message:raw_json!({"body":js_json::stringify_lossless(&answer),"id":message_id,"kind":"reply","status":"pending","fromAgentId":caller_id.unwrap(),"toAgentId":params.agent_id}), store_ref:RuntimeMailboxStoreRef { message_id, session_path:persistence.session_path }, updated_at:None
    }).map_err(handler)?;
    Ok(tool_result(
        JsString::from("Answered ") + &target.display_name + " (request " + &params.request_id + ").",
        json!({"agentId":params.agent_id,"requestId":params.request_id}),
    ))
}
pub fn register_question_relay_tools(pi: &ExtensionApi, store: Arc<MultiAgentStore>) -> Result<(), HandlerError> {
    let mut ask = ask_questions::create_ask_questions_tool_definition();
    ask.prompt_guidelines.as_mut().unwrap().push(
        "In a child agent, ask_questions sends the questions to your parent agent, which answers or asks the user."
            .into(),
    );
    let user_execute = ask.execute.clone();
    let ask_store = store.clone();
    ask.execute = Arc::new(move |id, params, signal, update, ctx| {
        let store = ask_store.clone();
        let user_execute = user_execute.clone();
        Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
            if ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()? {
                ask_parent(store, id.into(), params, signal, ctx).await
            } else {
                user_execute(id, params, signal, update, Some(ctx)).await
            }
        })
    });
    pi.register_tool(ask).map_err(handler)?;
    pi.register_tool(tool_definition("answer_question", "Answer Question", "Answer questions a direct child agent sent with ask_questions. A blocking child resumes with these answers as its ask_questions result.", None,
        json!({"type":"object","required":["agentId","requestId","answers"],"properties":{
            "agentId":{"type":"string","description":"Direct child agent that asked the questions."},
            "requestId":{"type":"string","description":"requestId from the child's question message."},
            "answers":{"type":"object","patternProperties":{"^.*$":{"type":"string"}},"description":"Answers keyed by exact question text. Omit unanswered questions."},
            "cancelled":{"type":"boolean","description":"True when declining to answer; the child receives a cancelled result."}},"additionalProperties":false}),
        Arc::new(move |_id, params, _signal, _update, ctx| { let store = store.clone(); Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?; answer_question(&store, js_json::from_value(&params).map_err(handler)?, &ctx) }) })
    )).map_err(handler)
}
