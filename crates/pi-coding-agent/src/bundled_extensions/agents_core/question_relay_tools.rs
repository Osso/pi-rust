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
use crate::core::session_manager::lock_session_manager;
use crate::core::tools::ask_questions::{
    self, AskQuestionsParams, Question, format_answers, parse_params, question_result, validate_questions,
};
use pi_agent_core::AgentToolResult;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn handler(error: impl std::fmt::Display) -> HandlerError {
    HandlerError::new(error.to_string())
}
fn answer_message_id(agent_id: &str, request_id: &str) -> String {
    format!("ask_questions:{agent_id}:{request_id}")
}
fn format_question(question: &Question, index: usize) -> String {
    let header = question
        .header
        .as_ref()
        .filter(|header| !header.is_empty())
        .map(|header| format!("[{header}] "))
        .unwrap_or_default();
    let mut lines = vec![format!("{}. {header}{}", index + 1, question.question)];
    let Some(options) = &question.options else {
        lines.push("   Free-text answer.".to_owned());
        return lines.join("\n");
    };
    let choice = if question.multi_select == Some(true) {
        "Choose one or more (comma-separated)"
    } else {
        "Choose one"
    };
    lines.push(format!("   {choice}, or give another answer:"));
    for option in options {
        let description = option
            .description
            .as_ref()
            .filter(|description| !description.is_empty())
            .map(|description| format!(" — {description}"))
            .unwrap_or_default();
        lines.push(format!("   - {}{description}", option.label));
    }
    lines.join("\n")
}
fn format_parent_request(agent_id: &str, request_id: &str, params: &AskQuestionsParams) -> String {
    let waiting = if params.blocking {
        "The child is blocked until you answer."
    } else {
        "The child continues meanwhile."
    };
    let mut lines = vec![
        format!("Child agent {agent_id} asks (requestId: {request_id}). {waiting}"),
        format!(
            "Reply with answer_question using agentId \"{agent_id}\" and requestId \"{request_id}\", answers keyed by exact question text. Ask the user with ask_questions first when you cannot answer."
        ),
        String::new(),
    ];
    lines.extend(
        params
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| format_question(question, index)),
    );
    lines.join("\n")
}
struct Inbox {
    agent_id: String,
    db: String,
    session_id: String,
}
fn send_questions_to_parent(
    store: &Arc<MultiAgentStore>,
    request_id: &str,
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
    if message["id"].as_str().is_none_or(str::is_empty) || message["status"] == "failed" {
        let reason = sent
            .content
            .iter()
            .filter_map(|part| match part {
                pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<String>();
        return Err(handler(format!("Could not send questions to parent: {reason}")));
    }
    let session = ctx.session_manager()?;
    let session_id = lock_session_manager(&session).get_session_id().to_owned();
    Ok(Inbox {
        agent_id,
        db,
        session_id,
    })
}
async fn wait_for_answer(
    inbox: &Inbox,
    request_id: &str,
    signal: Option<CancellationToken>,
) -> Result<Option<RuntimeMailboxMessage>, HandlerError> {
    let message_id = answer_message_id(&inbox.agent_id, request_id);
    let recipient = RuntimeMailboxAddress {
        agent_id: Some(inbox.agent_id.clone()),
        session_id: inbox.session_id.clone(),
    };
    loop {
        if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Ok(None);
        }
        let mut messages = take_runtime_mailbox_messages_for_delivery(
            &inbox.db,
            &recipient,
            |message| message.store_ref.message_id == message_id,
            1,
        )
        .map_err(handler)?;
        if let Some(message) = messages.pop() {
            return Ok(Some(message));
        }
        let delay = tokio::time::sleep(std::time::Duration::from_millis(500));
        if let Some(signal) = &signal {
            tokio::select! { _ = delay => {}, _ = signal.cancelled() => return Ok(None) }
        } else {
            delay.await;
        }
    }
}
async fn ask_parent(
    store: Arc<MultiAgentStore>,
    request_id: String,
    params: Value,
    signal: Option<CancellationToken>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let parsed = parse_params(&params).map_err(handler)?;
    validate_questions(&parsed.questions).map_err(handler)?;
    let inbox = send_questions_to_parent(&store, &request_id, &parsed, &ctx)?;
    if !parsed.blocking {
        let mut result = question_result(
            &params,
            format!(
                "Questions sent to parent (request {request_id}). Continue independent work; the answer will arrive in your inbox."
            ),
            json!({}),
            false,
        );
        let details = result.details.as_mut().unwrap();
        details["queued"] = json!(true);
        details["requestId"] = json!(request_id);
        return Ok(result);
    }
    let Some(delivered) = wait_for_answer(&inbox, &request_id, signal).await? else {
        return Ok(question_result(&params, "Questions cancelled", json!({}), true));
    };
    let answer: Value = serde_json::from_str(&delivered.body).map_err(handler)?;
    if !answer.is_object() || answer["type"] != "ask_questions_answer" {
        return Err(handler(format!(
            "Malformed question answer {}",
            delivered.store_ref.message_id
        )));
    }
    let cancelled = answer["cancelled"].as_bool().unwrap_or(false);
    let outcome = if cancelled { "cancelled" } else { "answered" };
    let mut result = question_result(
        &params,
        format!("Parent {outcome} questions:\n{}", format_answers(&answer["answers"])),
        answer["answers"].clone(),
        cancelled,
    );
    result.details.as_mut().unwrap()["runtimeMailboxDelivery"] =
        json!({"controlDbPath":inbox.db,"messages":[{"id":delivered.id,"storeRef":delivered.store_ref}]});
    Ok(result)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnswerQuestionParams {
    agent_id: String,
    request_id: String,
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
        .and_then(|target| target.parent_id.as_deref())
        .unwrap_or("main");
    if caller_id.is_none() || target.is_none() || Some(parent_id) != caller_id.as_deref() {
        return Err(handler(format!(
            "Could not answer {}: not a direct child of this agent",
            params.agent_id
        )));
    }
    let target = target.unwrap();
    if !is_active_lifecycle(target.lifecycle) {
        return Err(handler(format!(
            "Could not answer {}: inactive ({})",
            params.agent_id,
            serde_json::to_value(target.lifecycle)
                .map_err(handler)?
                .as_str()
                .unwrap_or("")
        )));
    }
    let persistence = store.get_persistence_target();
    let session_id = target
        .transcript
        .as_ref()
        .map(|transcript| transcript.session_id.clone());
    let db = ctx.control_db_path()?;
    let (Some(persistence), Some(session_id)) = (persistence, session_id) else {
        return Err(handler(format!(
            "Could not answer {}: runtime mailbox is unavailable",
            params.agent_id
        )));
    };
    if Some(persistence.control_db_path.as_str()) != db.as_deref() {
        return Err(handler(format!(
            "Could not answer {}: runtime mailbox is unavailable",
            params.agent_id
        )));
    }
    let answer = json!({"type":"ask_questions_answer","requestId":params.request_id,"answers":params.answers,"cancelled":params.cancelled == Some(true)});
    let message_id = answer_message_id(&params.agent_id, &params.request_id);
    let session = ctx.session_manager()?;
    let sender_session_id = lock_session_manager(&session).get_session_id().to_owned();
    enqueue_stored_runtime_mailbox_message(&persistence.control_db_path, &EnqueueStoredRuntimeMailboxMessageInput {
        recipient_host:None, recipient:RuntimeMailboxAddress { agent_id:Some(params.agent_id.clone()), session_id }, sender:RuntimeMailboxAddress { agent_id:ctx.multi_agent_agent_id()?, session_id:sender_session_id }, kind:RuntimeMailboxMessageKind::Reply,
        message:json!({"body":serde_json::to_string(&answer).map_err(handler)?,"id":message_id,"kind":"reply","status":"pending","fromAgentId":caller_id.unwrap(),"toAgentId":params.agent_id}), store_ref:RuntimeMailboxStoreRef { message_id, session_path:persistence.session_path }, updated_at:None
    }).map_err(handler)?;
    Ok(tool_result(
        format!("Answered {} (request {}).", target.display_name, params.request_id),
        json!({"agentId":params.agent_id,"requestId":params.request_id}),
    ))
}
pub fn register_question_relay_tools(pi: &ExtensionApi, store: Arc<MultiAgentStore>) -> Result<(), HandlerError> {
    let mut ask = ask_questions::create_ask_questions_tool_definition();
    ask.prompt_guidelines.as_mut().unwrap().push(
        "In a child agent, ask_questions sends the questions to your parent agent, which answers or asks the user."
            .to_owned(),
    );
    let user_execute = ask.execute.clone();
    let ask_store = store.clone();
    ask.execute = Arc::new(move |id, params, signal, update, ctx| {
        let store = ask_store.clone();
        let user_execute = user_execute.clone();
        Box::pin(async move {
            if ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()? {
                ask_parent(store, id, params, signal, ctx).await
            } else {
                user_execute(id, params, signal, update, ctx).await
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
        Arc::new(move |_id, params, _signal, _update, ctx| { let store = store.clone(); Box::pin(async move { answer_question(&store, serde_json::from_value(params).map_err(handler)?, &ctx) }) })
    )).map_err(handler)
}
