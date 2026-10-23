//! Shared output and isolated-options seam for the SDK programs.
use pi_agent_core::agent::types::AgentEvent;
use pi_ai::types::AssistantMessageEvent;
use pi_coding_agent::core::{
    agent_session::{AgentMessageEvent, AgentSessionEvent, PromptOptions},
    sdk::{CreateAgentSessionOptions, create_agent_session},
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn stream<W: Write + Send + 'static>(
    options: CreateAgentSessionOptions,
    prompt: &str,
    prefix: &str,
    suffix: &str,
    output: Arc<Mutex<W>>,
) -> Result<(), JsString> {
    let session = create_agent_session(options)
        .await
        .map_err(|error| error.message)?
        .session;
    let failure = Arc::new(Mutex::new(None));
    let stream_failure = Arc::clone(&failure);
    let stream_output = Arc::clone(&output);
    session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::Message(AgentMessageEvent {
            event: AgentEvent::MessageUpdate {
                assistant_message_event,
                ..
            },
            ..
        }) = event
            && let AssistantMessageEvent::TextDelta { delta, .. } = assistant_message_event.as_ref()
        {
            let mut failure = stream_failure.lock().expect("failure mutex poisoned");
            if failure.is_none() {
                let mut writer = stream_output.lock().expect("output mutex poisoned");
                if let Err(error) = writer
                    .write_all(encode_node_utf8(delta).as_bytes())
                    .and_then(|()| writer.flush())
                {
                    *failure = Some(JsString::from(error.to_string()));
                }
            }
        }
    }));
    let outcome = async {
        output
            .lock()
            .expect("output mutex poisoned")
            .write_all(prefix.as_bytes())
            .map_err(|e| JsString::from(e.to_string()))?;
        let prompt_result = session.prompt(prompt, PromptOptions::default()).await;
        complete_prompt(prompt_result, failure.lock().expect("failure mutex poisoned").take())?;
        let mut writer = output.lock().expect("output mutex poisoned");
        writer
            .write_all(suffix.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|e| JsString::from(e.to_string()))
    }
    .await;
    session.dispose();
    outcome
}

fn complete_prompt(
    prompt_result: Result<(), pi_coding_agent::core::agent_session::AgentSessionError>,
    writer_error: Option<JsString>,
) -> Result<(), JsString> {
    if let Some(error) = writer_error {
        return Err(error);
    }
    prompt_result.map_err(|error| error.message().clone())
}

#[cfg(test)]
mod tests {
    use super::complete_prompt;
    use pi_coding_agent::core::agent_session::AgentSessionError;

    #[test]
    fn first_writer_error_precedes_later_prompt_error() {
        assert_eq!(
            complete_prompt(
                Err(AgentSessionError::new("later prompt error")),
                Some("first writer error".into())
            ),
            Err("first writer error".into())
        );
        assert_eq!(
            complete_prompt(Err(AgentSessionError::new("prompt error")), None),
            Err("prompt error".into())
        );
        assert_eq!(complete_prompt(Ok(()), None), Ok(()));
    }
}
