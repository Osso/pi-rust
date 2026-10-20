//! Shared output and isolated-options seam for the SDK programs.
use pi_agent_core::agent::types::AgentEvent;
use pi_ai::types::AssistantMessageEvent;
use pi_coding_agent::core::{
    agent_session::{AgentMessageEvent, AgentSessionEvent, PromptOptions},
    sdk::{CreateAgentSessionOptions, create_agent_session},
};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn stream<W: Write + Send + 'static>(
    options: CreateAgentSessionOptions,
    prompt: &str,
    prefix: &str,
    suffix: &str,
    output: Arc<Mutex<W>>,
) -> Result<(), String> {
    let session = create_agent_session(options).await?.session;
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
            let mut writer = stream_output.lock().expect("output mutex poisoned");
            if let Err(error) = writer.write_all(delta.as_bytes()).and_then(|()| writer.flush()) {
                *stream_failure.lock().expect("failure mutex poisoned") = Some(error.to_string());
            }
        }
    }));
    let outcome = async {
        output
            .lock()
            .expect("output mutex poisoned")
            .write_all(prefix.as_bytes())
            .map_err(|e| e.to_string())?;
        session
            .prompt(prompt, PromptOptions::default())
            .await
            .map_err(|e| e.message().to_owned())?;
        if let Some(error) = failure.lock().expect("failure mutex poisoned").take() {
            return Err(error);
        }
        let mut writer = output.lock().expect("output mutex poisoned");
        writer
            .write_all(suffix.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|e| e.to_string())
    }
    .await;
    session.dispose();
    outcome
}
