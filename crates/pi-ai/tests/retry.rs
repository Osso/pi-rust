use std::future::Future;
use std::sync::{Arc, Mutex};

use pi_ai::types::{AssistantContent, AssistantMessage, StopReason, TextContent};
use pi_ai::utils::retry::{RetryCallbacks, RetryPolicy, is_retryable_assistant_error, retry_assistant_call};
use tokio_util::sync::CancellationToken;

fn message(stop_reason: StopReason, error_message: Option<&str>) -> AssistantMessage {
    AssistantMessage {
        stop_reason,
        error_message: error_message.map(Into::into),
        ..AssistantMessage::empty("openai-completions", "test", "model", 1.0)
    }
}

fn error(error_message: &str) -> AssistantMessage {
    message(StopReason::Error, Some(error_message))
}

fn recovered() -> AssistantMessage {
    let mut message = message(StopReason::Stop, None);
    message
        .content
        .push(AssistantContent::Text(TextContent::new("recovered")));
    message
}

#[derive(Default)]
struct Recorder {
    events: Arc<Mutex<Vec<String>>>,
}

impl RetryCallbacks for Recorder {
    fn on_retry_scheduled<'a>(
        &'a mut self,
        attempt: u32,
        max_attempts: u32,
        delay_ms: f64,
        error_message: &'a pi_js_compat::js_json::JsString,
    ) -> impl Future<Output = ()> + Send + 'a {
        let events = Arc::clone(&self.events);
        let error_message = error_message.to_utf8().unwrap();
        async move {
            events
                .lock()
                .unwrap()
                .push(format!("retry:{attempt}/{max_attempts}/{delay_ms}:{error_message}"));
        }
    }

    fn on_retry_attempt_start(&mut self) -> impl Future<Output = ()> + Send {
        let events = Arc::clone(&self.events);
        async move {
            events.lock().unwrap().push("attempt-start".to_owned());
        }
    }

    fn on_retry_finished(
        &mut self,
        success: bool,
        attempt: u32,
        final_error: Option<&pi_js_compat::js_json::JsString>,
    ) -> impl Future<Output = ()> + Send {
        let events = Arc::clone(&self.events);
        let final_error = final_error.map(|text| text.to_utf8().unwrap());
        async move {
            events.lock().unwrap().push(format!(
                "finished:{success}/{attempt}/{}",
                final_error.unwrap_or_else(|| "none".to_owned())
            ));
        }
    }
}

fn events(recorder: &Recorder) -> Vec<String> {
    recorder.events.lock().unwrap().clone()
}

fn policy(enabled: bool, max_retries: u32) -> RetryPolicy {
    RetryPolicy {
        enabled,
        max_retries,
        base_delay_ms: 0.0,
    }
}

#[test]
fn classifies_provider_retry_errors() {
    assert!(!is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: max_output_tokens"
    )));
    assert!(is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: unknown"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: content_filter"
    )));
    assert!(is_retryable_assistant_error(&error(
        "OpenAI Responses stream ended before a terminal response event"
    )));
    assert!(is_retryable_assistant_error(&error(
        "The server had an error while processing your request. Sorry about that!"
    )));
    assert!(is_retryable_assistant_error(&error(
        "An error occurred while processing your request. You can retry your request, or contact us through our help center at help.openai.com if the error persists. Please include the request ID req_******** in your message."
    )));
    assert!(is_retryable_assistant_error(&error(
        "{\"message\":\"The system encountered an unexpected error during processing. Try your request again.\"}"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "ResourceExhausted: Worker local total request limit reached (288/48)"
    )));
    assert!(is_retryable_assistant_error(&error(
        "The socket connection was closed unexpectedly. For more information, pass `verbose: true` in the second argument to fetch()"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "Error: exceeded request buffer limit while retrying upstream"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "The pending stream has been canceled (caused by: getaddrinfo ENOTFOUND bedrock-runtime.us-east-1.amazonaws.com)"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "connect ENOTFOUND api.example.com"
    )));
    assert!(!is_retryable_assistant_error(&error("EAI_AGAIN api.example.com")));
    assert!(!is_retryable_assistant_error(&error(
        "getaddrinfo failed for api.example.com"
    )));
    assert!(is_retryable_assistant_error(&error(
        "OpenAI Responses stream ended before a terminal response event"
    )));
    assert!(!is_retryable_assistant_error(&error("429 quota exceeded")));
    assert!(is_retryable_assistant_error(&error("overloaded_error")));
    assert!(is_retryable_assistant_error(&error("524 status code (no body)")));
    assert!(!is_retryable_assistant_error(&message(StopReason::Stop, None)));
    assert!(!is_retryable_assistant_error(&message(StopReason::Error, None)));
    assert!(!is_retryable_assistant_error(&error("insufficient_quota")));
    assert!(!is_retryable_assistant_error(&error("Monthly usage limit reached")));
    assert!(!is_retryable_assistant_error(&error("GoUsageLimitError")));
}

#[tokio::test]
async fn successful_response_returns_immediately_without_retrying() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            *counter.lock().unwrap() += 1;
            let message = recovered();
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Stop);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(events(&recorder).is_empty());
}

#[tokio::test]
async fn aborted_message_is_not_retried() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            *counter.lock().unwrap() += 1;
            let message = message(StopReason::Aborted, None);
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(events(&recorder).is_empty());
}

#[tokio::test]
async fn non_retryable_error_returns_immediately() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            *counter.lock().unwrap() += 1;
            let message = error("insufficient_quota");
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Error);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(events(&recorder).is_empty());
}

#[tokio::test]
async fn retries_transient_errors_up_to_max_retries_then_returns_the_final_error() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            *counter.lock().unwrap() += 1;
            let message = error("terminated");
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Error);
    assert_eq!(*calls.lock().unwrap(), 4);
    let events = events(&recorder);
    assert_eq!(events.iter().filter(|event| event.starts_with("retry:")).count(), 3);
    assert_eq!(events.last().unwrap(), "finished:false/3/terminated");
}

#[tokio::test]
async fn stops_retrying_once_a_call_succeeds() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            let mut count = counter.lock().unwrap();
            *count += 1;
            let message = if *count < 3 { error("terminated") } else { recovered() };
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.content.len(), 1);
    assert_eq!(*calls.lock().unwrap(), 3);
    assert_eq!(events(&recorder).last().unwrap(), "finished:true/2/none");
}

#[tokio::test]
async fn reports_an_aborted_retried_call_as_unsuccessful() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            let mut count = counter.lock().unwrap();
            *count += 1;
            let message = if *count == 1 {
                error("terminated")
            } else {
                message(StopReason::Aborted, None)
            };
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(events(&recorder).last().unwrap(), "finished:false/1/none");
}

#[tokio::test]
async fn disabled_policy_does_not_retry() {
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let response = retry_assistant_call(
        move || {
            *counter.lock().unwrap() += 1;
            let message = error("terminated");
            async move { message }
        },
        Some(&policy(false, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Error);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(events(&recorder).is_empty());
}

#[tokio::test]
async fn attempt_start_is_emitted_after_backoff_before_each_retried_call() {
    let sequence = Arc::new(Mutex::new(Vec::new()));
    let produce_sequence = Arc::clone(&sequence);
    let recorder = Recorder {
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let recorder_events = Arc::clone(&recorder.events);
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let mut recorder = recorder;
    let response = retry_assistant_call(
        move || {
            let mut count = counter.lock().unwrap();
            produce_sequence.lock().unwrap().push(format!("produce:{count}"));
            *count += 1;
            let message = if *count < 3 { error("terminated") } else { recovered() };
            async move { message }
        },
        Some(&policy(true, 3)),
        None,
        Some(&mut recorder),
    )
    .await;
    assert_eq!(response.stop_reason, StopReason::Stop);
    let events = recorder_events.lock().unwrap().clone();
    assert_eq!(events.iter().filter(|event| event.starts_with("retry:")).count(), 2);
    assert_eq!(events.iter().filter(|event| *event == "attempt-start").count(), 2);
}

#[tokio::test]
async fn aborting_backoff_returns_an_aborted_message_and_reports_failure() {
    let token = CancellationToken::new();
    let calls = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    let recorder = Recorder {
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let recorder_events = Arc::clone(&recorder.events);
    let mut recorder = recorder;
    let cancel = token.clone();
    let handle = tokio::spawn(async move {
        retry_assistant_call(
            move || {
                *counter.lock().unwrap() += 1;
                let message = error("terminated");
                async move { message }
            },
            Some(&RetryPolicy {
                enabled: true,
                max_retries: 5,
                base_delay_ms: 10_000.0,
            }),
            Some(&cancel),
            Some(&mut recorder),
        )
        .await
    });
    tokio::task::yield_now().await;
    token.cancel();
    let response = handle.await.expect("retry task must finish");
    assert_eq!(response.stop_reason, StopReason::Aborted);
    assert_eq!(response.error_message, None);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(
        recorder_events.lock().unwrap().last().unwrap(),
        "finished:false/1/terminated"
    );
}

#[test]
fn frozen_exact_transport_and_access_errors() {
    for text in [
        "WebSocket connection to 'wss://example.com' failed: Expected 101 status code",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.",
    ] {
        assert!(is_retryable_assistant_error(&error(text)), "{text}");
    }
    let mut blocked = error("This content was flagged for possible cybersecurity risk. 503");
    blocked.api = "openai-codex-responses".into();
    assert!(!is_retryable_assistant_error(&blocked));
}

#[test]
fn frozen_codex_cybersecurity_wins_over_transient_suffix() {
    for text in [
        "This content was flagged for possible cybersecurity risk. 503",
        "Codex error: This content was flagged for possible cybersecurity risk. timeout",
    ] {
        let mut blocked = error(text);
        blocked.api = "openai-codex-responses".into();
        assert!(!is_retryable_assistant_error(&blocked), "{text}");
    }
}

#[test]
fn frozen_regex_uses_javascript_characters() {
    for text in ["rate😀limit", "rate\u{2028}limit", "ſerver error", "socKet hang up"] {
        assert!(!is_retryable_assistant_error(&error(text)), "{text}");
    }
}
