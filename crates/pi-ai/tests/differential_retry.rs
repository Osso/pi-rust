#[path = "support/json_compare.rs"]
mod json_compare;
#[path = "support/oracle.rs"]
mod oracle;

use std::future::Future;
use std::sync::{Arc, Mutex};

use pi_ai::types::AssistantMessage;
use pi_ai::utils::retry::{RetryCallbacks, RetryPolicy, is_retryable_assistant_error, retry_assistant_call};
use serde_json::{Value, json};

fn usage() -> Value {
    json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}})
}

fn response(stop_reason: &str, error_message: Option<&str>, text: &str) -> Value {
    let mut value = json!({
        "role": "assistant",
        "content": if text.is_empty() { json!([]) } else { json!([{"type": "text", "text": text}]) },
        "api": "openai-completions",
        "provider": "test",
        "model": "model",
        "usage": usage(),
        "stopReason": stop_reason,
        "timestamp": 1
    });
    if let Some(message) = error_message {
        value["errorMessage"] = json!(message);
    }
    value
}

fn classification_messages() -> Vec<Value> {
    [
        "Incomplete response returned, reason: max_output_tokens",
        "Incomplete response returned, reason: unknown",
        "Incomplete response returned, reason: content_filter",
        "OpenAI Responses stream ended before a terminal response event",
        "The server had an error while processing your request. Sorry about that!",
        "An error occurred while processing your request. You can retry your request, or contact us through our help center at help.openai.com if the error persists. Please include the request ID req_******** in your message.",
        "{\"message\":\"The system encountered an unexpected error during processing. Try your request again.\"}",
        "ResourceExhausted: Worker local total request limit reached (288/48)",
        "The socket connection was closed unexpectedly. For more information, pass `verbose: true` in the second argument to fetch()",
        "Error: exceeded request buffer limit while retrying upstream",
        "The pending stream has been canceled (caused by: getaddrinfo ENOTFOUND bedrock-runtime.us-east-1.amazonaws.com)",
        "connect ENOTFOUND api.example.com",
        "EAI_AGAIN api.example.com",
        "getaddrinfo failed for api.example.com",
        "429 quota exceeded",
        "overloaded_error",
        "524 status code (no body)",
        "insufficient_quota",
        "Monthly usage limit reached",
        "GoUsageLimitError",
        "FreeUsageLimitError",
        "available balance",
        "out of budget",
        "quota exceeded",
        "billing",
        "rate limit",
        "too many requests",
        "service unavailable",
        "connection refused",
        "other side closed",
        "fetch failed",
        "upstream connect error",
        "reset before headers",
        "socket hang up",
        "closed unexpectedly",
        "timed out",
        "timeout",
        "terminated",
        "websocket closed",
        "websocket error",
        "ended without",
        "stream ended before message_stop",
        "http2 request did not get a response",
        "retry delay",
        "please retry your request",
    ]
    .into_iter()
    .map(|message| response("error", Some(message), ""))
    .collect()
}

fn requests() -> Vec<Value> {
    let mut requests: Vec<Value> = classification_messages()
        .into_iter()
        .enumerate()
        .map(|(id, message)| json!({"id": id, "operation": "classify", "message": message}))
        .collect();
    let base = requests.len();
    let policy =
        |enabled: bool, max_retries: u32| json!({"enabled": enabled, "maxRetries": max_retries, "baseDelayMs": 0});
    requests.push(json!({"id": base, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("stop", None, "ok")]}));
    requests.push(json!({"id": base + 1, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("error", Some("terminated"), ""), response("error", Some("terminated"), ""),
                      response("stop", None, "recovered")]}));
    requests.push(json!({"id": base + 2, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("error", Some("terminated"), "")]}));
    requests.push(json!({"id": base + 3, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("error", Some("insufficient_quota"), "")]}));
    requests.push(json!({"id": base + 4, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("aborted", None, "")]}));
    requests.push(json!({"id": base + 5, "operation": "retry", "policy": policy(false, 3),
        "responses": [response("error", Some("terminated"), "")]}));
    requests.push(json!({"id": base + 6, "operation": "retry", "policy": policy(true, 0),
        "responses": [response("error", Some("terminated"), "")]}));
    requests.push(json!({"id": base + 7, "operation": "retry", "policy": null,
        "responses": [response("error", Some("terminated"), "")]}));
    requests.push(json!({"id": base + 8, "operation": "retry", "policy": policy(true, 2),
        "responses": [response("error", Some("terminated"), ""), response("aborted", None, "")]}));
    requests.push(json!({"id": base + 9, "operation": "retry", "policy": policy(true, 3),
        "responses": [response("error", Some(""), "")]}));
    requests
}

#[derive(Default)]
struct Recorder {
    events: Arc<Mutex<Vec<Value>>>,
}

impl RetryCallbacks for Recorder {
    fn on_retry_scheduled<'a>(
        &'a mut self,
        attempt: u32,
        max_attempts: u32,
        delay_ms: f64,
        error_message: &'a str,
    ) -> impl Future<Output = ()> + Send + 'a {
        let events = Arc::clone(&self.events);
        async move {
            events.lock().unwrap().push(json!({
                "kind": "scheduled", "attempt": attempt, "maxAttempts": max_attempts,
                "delayMs": delay_ms, "errorMessage": error_message
            }));
        }
    }

    fn on_retry_attempt_start(&mut self) -> impl Future<Output = ()> + Send {
        let events = Arc::clone(&self.events);
        async move {
            events.lock().unwrap().push(json!({"kind": "attempt-start"}));
        }
    }

    fn on_retry_finished(
        &mut self,
        success: bool,
        attempt: u32,
        final_error: Option<&str>,
    ) -> impl Future<Output = ()> + Send {
        let events = Arc::clone(&self.events);
        let final_error = final_error.map(str::to_owned);
        async move {
            events.lock().unwrap().push(json!({
                "kind": "finished", "success": success, "attempt": attempt, "finalError": final_error
            }));
        }
    }
}

async fn observe_retry(request: &Value) -> Value {
    let policy: Option<RetryPolicy> = request["policy"].as_object().map(|policy| RetryPolicy {
        enabled: policy["enabled"].as_bool().expect("enabled"),
        max_retries: policy["maxRetries"].as_u64().expect("maxRetries") as u32,
        base_delay_ms: policy["baseDelayMs"].as_f64().expect("baseDelayMs"),
    });
    let responses: Vec<AssistantMessage> = request["responses"]
        .as_array()
        .expect("responses")
        .iter()
        .map(|response| serde_json::from_value(response.clone()).expect("response"))
        .collect();
    let calls = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&calls);
    let mut recorder = Recorder::default();
    let final_message = retry_assistant_call(
        move || {
            let mut count = counter.lock().unwrap();
            let response = responses[(*count).min(responses.len() - 1)].clone();
            *count += 1;
            async move { response }
        },
        policy.as_ref(),
        None,
        Some(&mut recorder),
    )
    .await;
    let events = recorder.events.lock().unwrap().clone();
    json!({
        "id": request["id"], "ok": true,
        "output": {"final": final_message, "events": events, "calls": *calls.lock().unwrap()}
    })
}

fn observe_classify(request: &Value) -> Value {
    let message: AssistantMessage = serde_json::from_value(request["message"].clone()).expect("message");
    json!({
        "id": request["id"], "ok": true,
        "output": {"retryable": is_retryable_assistant_error(&message)}
    })
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Number(number) => json!(number.as_f64().expect("number")),
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(object) => {
            Value::Object(object.into_iter().map(|(key, value)| (key, canonical(value))).collect())
        }
        other => other,
    }
}

#[tokio::test]
async fn retry_classification_and_loops_match_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("retry-oracle.ts", "packages/ai/src/utils/retry.ts");
    for request in requests() {
        let observed = if request["operation"] == "classify" {
            observe_classify(&request)
        } else {
            observe_retry(&request).await
        };
        let expected = oracle.request(&request);
        json_compare::assert_json_eq_ordered(
            &canonical(observed),
            &canonical(expected),
            &format!("request {request}"),
        );
    }
}
