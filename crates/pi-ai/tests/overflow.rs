use pi_ai::types::{AssistantMessage, StopReason, Usage};
use pi_ai::utils::overflow::{get_overflow_patterns, is_context_overflow};

fn error_message(message: &str) -> AssistantMessage {
    AssistantMessage {
        stop_reason: StopReason::Error,
        error_message: Some(message.to_owned().into()),
        ..AssistantMessage::empty("openai-completions", "ollama", "qwen3.5:35b", 1)
    }
}

fn length_stop(input: f64, cache_read: f64, cache_write: f64, output: f64) -> AssistantMessage {
    AssistantMessage {
        usage: Usage {
            input,
            output,
            cache_read,
            cache_write,
            total_tokens: input + cache_read + cache_write + output,
            ..Usage::default()
        },
        stop_reason: StopReason::Length,
        ..AssistantMessage::empty("openai-completions", "test-provider", "test-model", 1)
    }
}

#[test]
fn detects_explicit_ollama_prompt_too_long_errors() {
    let message = error_message("400 `prompt too long; exceeded max context length by 100918 tokens`");
    assert!(is_context_overflow(&message, Some(32_768.0)));
}

#[test]
fn detects_upstream_request_buffer_limit_errors_as_overflow() {
    let message = error_message("Error: exceeded request buffer limit while retrying upstream");
    assert!(is_context_overflow(&message, Some(32_768.0)));
}

#[test]
fn detects_together_and_litellm_context_length_errors() {
    let together =
        error_message("400 The input (516368 tokens) is longer than the model's context length (262144 tokens).");
    assert!(is_context_overflow(&together, Some(262_144.0)));

    let litellm = error_message(
        "Error: 503 litellm.ServiceUnavailableError: litellm.MidStreamFallbackError: litellm.APIConnectionError: APIConnectionError: OpenAIException - Requested token count exceeds the model's maximum context length of 131072 tokens.",
    );
    assert!(is_context_overflow(&litellm, Some(131_072.0)));
}

#[test]
fn detects_openai_and_openrouter_poolside_wording() {
    let openai = error_message("Error: 400 Input length (265330) exceeds model's maximum context length (262144).");
    assert!(is_context_overflow(&openai, Some(262_144.0)));

    let poolside = error_message(
        "Provider returned error: Input length 131393 exceeds the maximum allowed input length of 131040 tokens.",
    );
    assert!(is_context_overflow(&poolside, Some(131_072.0)));
}

#[test]
fn detects_ds4_configured_context_size_errors_with_commas() {
    let plain = error_message("400 Prompt has 256468 tokens, but the configured context size is 256000 tokens");
    assert!(is_context_overflow(&plain, Some(256_000.0)));

    let commas = error_message("Prompt has 5,958,968 tokens, but the configured context size is 256,000 tokens");
    assert!(is_context_overflow(&commas, Some(256_000.0)));
}

#[test]
fn detects_cerebras_no_body_status_errors() {
    assert!(is_context_overflow(&error_message("400 status code (no body)"), None));
    assert!(is_context_overflow(&error_message("413 (no body)"), None));
    assert!(!is_context_overflow(&error_message("500 (no body)"), None));
}

#[test]
fn does_not_treat_generic_errors_as_overflow() {
    assert!(!is_context_overflow(
        &error_message("500 `model runner crashed unexpectedly`"),
        Some(32_768.0)
    ));
}

#[test]
fn does_not_treat_bedrock_throttling_as_overflow() {
    let throttling = error_message("Throttling error: Too many tokens, please wait before trying again.");
    assert!(!is_context_overflow(&throttling, Some(200_000.0)));

    let unavailable = error_message("Service unavailable: The service is temporarily unavailable.");
    assert!(!is_context_overflow(&unavailable, Some(200_000.0)));
}

#[test]
fn does_not_treat_rate_limits_as_overflow() {
    assert!(!is_context_overflow(
        &error_message("Rate limit exceeded, please retry after 30 seconds."),
        Some(200_000.0)
    ));
    assert!(!is_context_overflow(
        &error_message("Too many requests. Please slow down."),
        Some(200_000.0)
    ));
}

#[test]
fn detects_xiaomi_style_length_stop_with_zero_output_and_filled_context() {
    let message = length_stop(58.0, 1_048_512.0, 0.0, 0.0);
    assert!(is_context_overflow(&message, Some(1_048_576.0)));
}

#[test]
fn normal_length_stops_are_not_context_overflow() {
    let with_output = length_stop(1_000.0, 0.0, 0.0, 4_096.0);
    assert!(!is_context_overflow(&with_output, Some(200_000.0)));

    let below_context = length_stop(100.0, 0.0, 0.0, 0.0);
    assert!(!is_context_overflow(&below_context, Some(200_000.0)));
}

#[test]
fn range_input_wording_is_not_in_frozen_overflow_patterns() {
    assert!(!is_context_overflow(
        &error_message("Range of input length should be [1, 100]"),
        None
    ));
}

#[test]
fn negative_context_window_is_truthy_like_javascript() {
    let mut message = length_stop(1.0, 0.0, 0.0, 1.0);
    message.stop_reason = StopReason::Stop;
    assert!(is_context_overflow(&message, Some(-1.0)));
    assert!(!is_context_overflow(&message, Some(0.0)));
}

#[test]
fn exposes_the_overflow_patterns_for_testing() {
    let patterns = get_overflow_patterns();
    assert_eq!(patterns.len(), 25);
    assert!(patterns.iter().any(|pattern| {
        pattern
            .find("prompt is too long: 213462 tokens > 200000 maximum")
            .is_some()
    }));
    assert!(
        patterns
            .iter()
            .any(|pattern| pattern.find("400 status code (no body)").is_some())
    );
}
