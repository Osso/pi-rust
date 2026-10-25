//! Runtime mailbox and shared-channel prompt formatting (`core/runtime-coordination-format.ts`).

use pi_js_compat::js_json::{JsString, JsValue, is_truthy, to_js_string};

use crate::core::session_control_db::{RuntimeMailboxMessage, SharedChannelMessage};

const SHARED_CHANNEL_RECEIVE_GUIDANCE: &str = concat!(
    "Shared-channel handling:\n",
    "Treat these messages as action-only coordination, not conversation.\n",
    "Do not acknowledge, restate, relay, summarize, or review channel messages.\n",
    "Do not confirm, praise, or classify them through channel_post.\n",
    "Take no action unless the message requests action from you or its affected path or artifact overlaps your work.\n",
    "Never echo diagnostic or test messages onto the shared channel. The same prohibition covers experiments, probes, and tag tests.\n",
    "Correct misuse only when needed to stop an immediate or repeated coordination hazard, using one terse targeted command."
);

/// `formatRuntimeMailboxPrompt`. Invalid source-created file references report their formatting failure.
pub fn format_runtime_mailbox_prompt(
    message: &RuntimeMailboxMessage,
    recipient_session_id: impl Into<JsString>,
) -> Result<JsString, JsString> {
    let mut sections = vec!["From:".into()];
    let host = message.sender_host.as_ref().filter(|host| !host.is_empty());
    if let Some(host) = host {
        sections.push(host_line(host));
    }
    sections.extend(sender_lines(
        &message.sender.session_id,
        message.sender.agent_id.as_ref(),
        &recipient_session_id.into(),
        host.is_some(),
    ));
    sections.extend(["".into(), "Message:".into()]);
    sections.push(if message.body.trim().is_empty() {
        "No message body.".into()
    } else {
        message.body.clone()
    });
    sections.extend(format_file_references(message.file_refs.as_ref())?);
    Ok(JsString::join(sections, "\n"))
}

/// `formatSharedChannelPrompt`.
pub fn format_shared_channel_prompt(
    messages: &[SharedChannelMessage],
    recipient_session_id: impl Into<JsString>,
) -> JsString {
    let recipient = recipient_session_id.into();
    let formatted = JsString::join(
        messages
            .iter()
            .map(|message| shared_channel_message(message, &recipient)),
        "\n\n",
    );
    formatted + "\n\n" + SHARED_CHANNEL_RECEIVE_GUIDANCE
}

fn shared_channel_message(message: &SharedChannelMessage, recipient_session_id: &JsString) -> JsString {
    let mut sections = vec!["From shared channel:".into()];
    let host = message.origin_host.as_ref().filter(|host| !host.is_empty());
    if let Some(host) = host {
        sections.push(host_line(&host.into()));
    }
    sections.extend(sender_lines(
        &message.sender.session_id,
        message.sender.agent_id.as_ref(),
        recipient_session_id,
        host.is_some(),
    ));
    let body = JsString::from(&message.body).trim();
    sections.extend([
        "".into(),
        "Message:".into(),
        if body.is_empty() {
            "No message body.".into()
        } else {
            body
        },
    ]);
    JsString::join(sections, "\n")
}

fn host_line(host: &JsString) -> JsString {
    JsString::from("- host: ") + host + " (reply with toHost and toSessionId)"
}

fn sender_lines(
    session_id: &JsString,
    agent_id: Option<&JsString>,
    recipient: &JsString,
    has_host: bool,
) -> Vec<JsString> {
    let session = if session_id.is_empty() {
        "unknown-session".into()
    } else {
        session_id.clone()
    };
    let agent = agent_id
        .filter(|agent| !agent.is_empty())
        .cloned()
        .unwrap_or_else(|| "main".into());
    let agent_line = JsString::from("- agent: ") + agent;
    if session == *recipient && !has_host {
        vec![agent_line]
    } else {
        vec![JsString::from("- session: ") + session, agent_line]
    }
}

fn format_file_references(file_refs: Option<&JsValue>) -> Result<Vec<JsString>, JsString> {
    let Some(file_refs) = file_refs.filter(|value| !matches!(value, JsValue::Null)) else {
        return Ok(Vec::new());
    };
    let refs = file_refs
        .as_array()
        .ok_or_else(|| JsString::from("message.fileRefs?.map is not a function"))?;
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines = vec!["Attached files:".into()];
    for file_ref in refs {
        if matches!(file_ref, JsValue::Null) {
            return Err("Cannot read properties of null (reading 'label')".into());
        }
        let label = match file_ref.get("label").filter(|label| is_truthy(label)) {
            Some(label) => to_js_string(label).map_err(JsString::from)? + " — ",
            None => JsString::default(),
        };
        let path = match file_ref.get("path") {
            Some(path) => to_js_string(path).map_err(JsString::from)?,
            None => "undefined".into(),
        };
        lines.push(JsString::from("- ") + label + path);
    }
    Ok(lines)
}
