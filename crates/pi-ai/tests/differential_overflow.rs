use crate::shared_oracle as oracle;

use pi_ai::types::AssistantMessage;
use pi_ai::utils::overflow::{get_overflow_patterns, is_context_overflow};
use serde_json::{Value, json};

fn error_messages() -> Vec<&'static str> {
    vec![
        "prompt is too long: 213462 tokens > 200000 maximum",
        "413 {\"error\":{\"type\":\"request_too_large\",\"message\":\"Request exceeds the maximum size\"}}",
        "Error: exceeded request buffer limit while retrying upstream",
        "The input is too long for requested model",
        "Your input exceeds the context window of this model",
        "Requested token count exceeds the model's maximum context length of 131072 tokens",
        "Error: 400 Input length (265330) exceeds model's maximum context length (262144).",
        "The input token count (1196265) exceeds the maximum number of tokens allowed (1048575)",
        "This model's maximum prompt length is 131072 but the request contains 537812 tokens",
        "Please reduce the length of the messages or completion",
        "This endpoint's maximum context length is 200000 tokens. However, you requested about 250000 tokens",
        "Input length 131393 exceeds the maximum allowed input length of 131040 tokens.",
        "The input (516368 tokens) is longer than the model's context length (262144 tokens).",
        "prompt token count of 300000 exceeds the limit of 200000",
        "the request exceeds the available context size, try increasing it",
        "tokens to keep from the initial prompt is greater than the context length",
        "invalid params, context window exceeds limit",
        "Your request exceeded model token limit: 300000 (requested: 300000)",
        "Prompt contains 300000 tokens ... too large for model with 200000 maximum context length",
        "Prompt has 5,958,968 tokens, but the configured context size is 256,000 tokens",
        "model_context_window_exceeded",
        "prompt too long; exceeded max context length by 100918 tokens",
        "Range of input length should be [1, 200000]",
        "context_length_exceeded",
        "too many tokens",
        "token limit exceeded",
        "400 status code (no body)",
        "413 (no body)",
        "500 (no body)",
        "Throttling error: Too many tokens, please wait before trying again.",
        "Service unavailable: The service is temporarily unavailable.",
        "Rate limit exceeded, please retry after 30 seconds.",
        "Too many requests. Please slow down.",
        "500 `model runner crashed unexpectedly`",
        "",
        "prompt iſ too long",
        "maximum prompt length is ١٢٣",
        "413\x0b(no body)",
    ]
}

fn assistant_json(message: &str, stop_reason: &str, input: u32, cache_read: u32, output: u32) -> Value {
    json!({
        "role": "assistant",
        "content": [],
        "api": "openai-completions",
        "provider": "test-provider",
        "model": "test-model",
        "usage": {"input": input, "output": output, "cacheRead": cache_read, "cacheWrite": 0,
                  "totalTokens": input + cache_read + output,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": stop_reason,
        "errorMessage": message,
        "timestamp": 1
    })
}

fn check_requests() -> Vec<Value> {
    let mut requests = Vec::new();
    let mut id = 0;
    for message in error_messages() {
        requests.push(json!({"id": id, "operation": "check",
            "message": assistant_json(message, "error", 0, 0, 0), "contextWindow": 200_000}));
        id += 1;
    }
    for window in [Value::Null, json!(0), json!(-1), json!(32_768)] {
        requests.push(json!({"id": id, "operation": "check",
            "message": assistant_json("", "stop", 40_000, 0, 10), "contextWindow": window}));
        id += 1;
    }
    for (input, cache_read, output, window) in [
        (58, 1_048_512, 0, 1_048_576),
        (1_000, 0, 4_096, 200_000),
        (100, 0, 0, 200_000),
        (3, 253_584, 16, 128_000),
        (4_062, 0, 1_024, 1_024),
    ] {
        requests.push(json!({"id": id, "operation": "check",
            "message": assistant_json("", "length", input, cache_read, output),
            "contextWindow": window}));
        id += 1;
    }
    requests.push(json!({"id": id, "operation": "patterns", "samples": error_messages()}));
    requests
}

fn observe(request: &Value) -> Value {
    if request["operation"] == "patterns" {
        let samples: Vec<&str> = request["samples"]
            .as_array()
            .expect("samples")
            .iter()
            .map(|sample| sample.as_str().expect("sample"))
            .collect();
        let matches: Vec<Vec<bool>> = get_overflow_patterns()
            .iter()
            .map(|pattern| samples.iter().map(|sample| pattern.find(sample).is_some()).collect())
            .collect();
        return json!({"id": request["id"], "ok": true, "output": matches});
    }

    let message: AssistantMessage = serde_json::from_value(request["message"].clone()).expect("message");
    let context_window = request["contextWindow"].as_f64();
    let output = json!({"overflow": is_context_overflow(&message, context_window)});
    json!({"id": request["id"], "ok": true, "output": output})
}

#[test]
fn overflow_detection_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("overflow-oracle.ts", "packages/ai/src/utils/overflow.ts");
    for request in check_requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}
