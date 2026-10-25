//! Frozen built-in Promise observers: abort settles the observer, not its producer.

use pi_agent_core::agent::{AgentToolResult, ToolError};
use pi_js_compat::abort::AbortSignal;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

type Completion = Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<AgentToolResult, ToolError>>>>>;

pub async fn observe_tool_producer<F>(producer: F, signal: Option<AbortSignal>) -> Result<AgentToolResult, ToolError>
where
    F: Future<Output = Result<AgentToolResult, ToolError>> + Send + 'static,
{
    let Some(signal) = signal else {
        return producer.await;
    };
    if signal.aborted() {
        return Err(ToolError::new("Operation aborted"));
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let completion = Arc::new(Mutex::new(Some(sender)));
    let abort_completion = completion.clone();
    let subscription = signal
        .subscribe(move |_| {
            settle(&abort_completion, Err(ToolError::new("Operation aborted")));
        })
        .map_err(|_| ToolError::new("Operation aborted"))?;
    tokio::spawn(async move {
        let result = producer.await;
        drop(subscription);
        settle(&completion, result);
    });
    receiver
        .await
        .map_err(|error| ToolError::new(format!("Tool operation failed to settle: {error}")))?
}

fn settle(completion: &Completion, result: Result<AgentToolResult, ToolError>) {
    if let Some(sender) = completion.lock().unwrap_or_else(PoisonError::into_inner).take() {
        // Later JS Promise settlements and delivery to a dropped observer are no-ops.
        let _ = sender.send(result);
    }
}
