//! Minimal SDK usage: discover resources and choose a model using SDK defaults.
use pi_js_compat::js_json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::Write;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::AgentEvent;
use pi_ai::types::AssistantMessageEvent;
use pi_coding_agent::core::agent_session::{AgentMessageEvent, AgentSessionEvent, PromptOptions};
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};

/// Run the example; explicit options and an owned writer permit isolated SDK fixtures.
/// Messages are JSON lines, preserving Node `console.log` values, not inspect syntax.
pub async fn run(
    options: CreateAgentSessionOptions,
    output: impl Write + Send + 'static,
) -> Result<(), pi_ai::request::CallbackError> {
    let result = create_agent_session(options).await?;
    let session = result.session;
    let output = Arc::new(Mutex::new(Ok(output)));
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
            let mut output = stream_output.lock().expect("output mutex poisoned");
            if let Ok(writer) = output.as_mut()
                && let Err(error) = writer
                    .write_all(encode_node_utf8(delta).as_bytes())
                    .and_then(|()| writer.flush())
            {
                *output = Err(error);
            }
        }
    }));
    let outcome = async {
        session
            .prompt("What files are in the current directory?", PromptOptions::default())
            .await
            .map_err(pi_ai::request::CallbackError::from)?;
        let mut output = output.lock().expect("output mutex poisoned");
        let writer = output.as_mut().map_err(|error| JsString::from(error.to_string()))?;
        for message in session.messages() {
            let line = js_json::to_value(&message).map_err(|error| JsString::from(error.to_string()))?;
            writeln!(writer, "{}", js_json::stringify_lossless(&line))
                .map_err(|error| JsString::from(error.to_string()))?;
        }
        writeln!(writer).map_err(|error| JsString::from(error.to_string()))?;
        writer
            .flush()
            .map_err(|error| pi_ai::request::CallbackError::from_error(JsString::from(error.to_string()), error))
    }
    .await;
    // Rust equivalent of the source's finally, including prompt/output failures.
    session.dispose();
    outcome
}

mod error;
pub use error::ExampleError;

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), ExampleError> {
    run(CreateAgentSessionOptions::default(), std::io::stdout())
        .await
        .map_err(ExampleError::new)
}
