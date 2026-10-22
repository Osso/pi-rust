use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, Usage, UsageCost};
use pi_coding_agent::core::cache_stats::{
    CacheWasteTotals, ModelPrice, ModelPriceSource, collect_cache_misses, compute_cache_waste, detect_cache_miss,
};
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, SessionMessageEntry};
use pi_js_compat::js_json::JsString;

struct Models;

impl ModelPriceSource for Models {
    fn get_model(&self, _provider: &str, _model_id: &str) -> Option<ModelPrice> {
        Some(ModelPrice { cache_read: 0.3 })
    }
}

struct Options {
    input: f64,
    cache_read: f64,
    cache_write: f64,
    cost: UsageCost,
    model: Option<String>,
    timestamp: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            input: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cost: UsageCost::default(),
            model: None,
            timestamp: 0,
        }
    }
}

fn assistant(options: Options) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(""))],
        api: "anthropic-messages".to_owned(),
        provider: "test".to_owned(),
        model: options.model.unwrap_or_else(|| "test-model".to_owned()),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage {
            input: options.input,
            output: 10.0,
            cache_read: options.cache_read,
            cache_write: options.cache_write,
            total_tokens: 0.0,
            cost: options.cost,
            ..Usage::default()
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: options.timestamp as f64,
    }))
}

fn entry(message: AgentMessage) -> SessionEntry {
    SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
        mailbox_store_refs: None,
        id: JsString::from("x"),
        parent_id: None,
        timestamp: JsString::default(),
        message: Box::new(message),
    }))
}

fn turn1() -> AgentMessage {
    assistant(Options {
        cache_write: 100_000.0,
        cost: UsageCost {
            cache_write: 0.375,
            ..UsageCost::default()
        },
        timestamp: 0,
        ..Options::default()
    })
}

fn turn2() -> AgentMessage {
    assistant(Options {
        cache_read: 100_000.0,
        cache_write: 5_000.0,
        cost: UsageCost {
            cache_read: 0.03,
            cache_write: 0.019,
            ..UsageCost::default()
        },
        timestamp: 60_000,
        ..Options::default()
    })
}

fn miss_turn(timestamp: i64) -> AgentMessage {
    assistant(Options {
        cache_write: 110_000.0,
        cost: UsageCost {
            cache_write: 0.4125,
            ..UsageCost::default()
        },
        timestamp,
        ..Options::default()
    })
}

#[test]
fn message_clock_keeps_fractional_and_nonfinite_idle_values() {
    for (previous_timestamp, timestamp, expected) in [
        (12.25, 12.75, 0.5),
        (12.75, 12.25, 0.0),
        (f64::NAN, 12.75, f64::NAN),
        (12.25, f64::NAN, f64::NAN),
        (12.25, f64::INFINITY, f64::INFINITY),
    ] {
        let AgentMessage::Assistant(mut previous) = turn1() else {
            panic!("assistant")
        };
        previous.timestamp = previous_timestamp;
        let AgentMessage::Assistant(mut current) = miss_turn(1) else {
            panic!("assistant")
        };
        current.timestamp = timestamp;
        let miss = detect_cache_miss(&[entry(AgentMessage::Assistant(previous))], &current, &Models).unwrap();
        if expected.is_nan() {
            assert!(miss.idle_ms.is_nan());
        } else {
            assert_eq!(miss.idle_ms, expected);
        }
    }
}

#[test]
fn accumulates_missed_tokens_and_cost_across_turns() {
    let totals = compute_cache_waste(&[entry(turn1()), entry(turn2()), entry(miss_turn(120_000))], &Models);
    assert_eq!(totals.missed_tokens, 105_000.0);
    assert!((totals.missed_cost - 0.36225).abs() < 1e-5);
    assert_eq!(totals.miss_count, 1);
}

#[test]
fn counts_nothing_for_healthy_sessions() {
    let totals = compute_cache_waste(&[entry(turn1()), entry(turn2())], &Models);
    assert_eq!(totals, CacheWasteTotals::default());
}

#[test]
fn skips_the_turn_after_a_compaction_reset() {
    let reset = SessionEntry::Known(KnownSessionEntry::Compaction(
        pi_coding_agent::core::session_manager::CompactionEntry {
            id: JsString::from("c"),
            parent_id: None,
            timestamp: JsString::default(),
            summary: JsString::default(),
            first_kept_entry_id: JsString::default(),
            tokens_before: 0.0,
            extra: Default::default(),
            duration_ms: None,
            details: None,
            provider_native: None,
            from_hook: None,
            first_kept_entry_index: None,
        },
    ));
    let after_reset = assistant(Options {
        cache_write: 20_000.0,
        cost: UsageCost {
            cache_write: 0.075,
            ..UsageCost::default()
        },
        ..Options::default()
    });
    let totals = compute_cache_waste(&[entry(turn1()), reset, entry(after_reset)], &Models);
    assert_eq!(totals.missed_tokens, 0.0);
}

#[test]
fn counts_misses_caused_by_model_switches() {
    let other_model = assistant(Options {
        cache_write: 100_000.0,
        cost: UsageCost {
            cache_write: 0.375,
            ..UsageCost::default()
        },
        model: Some("other-model".to_owned()),
        ..Options::default()
    });
    let totals = compute_cache_waste(&[entry(turn1()), entry(other_model)], &Models);
    assert_eq!(totals.missed_tokens, 100_000.0);
    assert_eq!(totals.miss_count, 1);
}

#[test]
fn skips_providers_that_report_no_cache_activity() {
    let a = assistant(Options {
        input: 100_000.0,
        ..Options::default()
    });
    let b = assistant(Options {
        input: 110_000.0,
        ..Options::default()
    });
    let totals = compute_cache_waste(&[entry(a), entry(b)], &Models);
    assert_eq!(totals.missed_tokens, 0.0);
}

#[test]
fn maps_counted_misses_to_their_entry_indexes() {
    let entries = vec![entry(turn1()), entry(turn2()), entry(miss_turn(120_000))];
    let misses = collect_cache_misses(&entries, &Models);
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0].0, 2);
    assert_eq!(misses[0].1.missed_tokens, 105_000.0);
}

#[test]
fn detects_a_miss_on_a_just_completed_message_with_idle_time() {
    let entries = vec![entry(turn1()), entry(turn2())];
    let message = miss_turn(600_000);
    let AgentMessage::Assistant(message) = &message else {
        panic!("expected assistant message");
    };
    let miss = detect_cache_miss(&entries, message, &Models).unwrap();
    assert_eq!(miss.missed_tokens, 105_000.0);
    assert!((miss.missed_cost - 0.36225).abs() < 1e-5);
    assert_eq!(miss.idle_ms, 540_000.0);
    assert!(!miss.model_changed);
}

#[test]
fn flags_model_switches_on_detected_misses() {
    let entries = vec![entry(turn1()), entry(turn2())];
    let other = assistant(Options {
        cache_write: 110_000.0,
        cost: UsageCost {
            cache_write: 0.4125,
            ..UsageCost::default()
        },
        model: Some("other-model".to_owned()),
        timestamp: 120_000,
        ..Options::default()
    });
    let AgentMessage::Assistant(other) = &other else {
        panic!("expected assistant message");
    };
    let miss = detect_cache_miss(&entries, other, &Models).unwrap();
    assert_eq!(miss.missed_tokens, 105_000.0);
    assert!(miss.model_changed);
}

#[test]
fn returns_none_for_healthy_turns_and_the_first_turn() {
    let entries = vec![entry(turn1()), entry(turn2())];
    let healthy = assistant(Options {
        cache_read: 105_000.0,
        cache_write: 2_000.0,
        cost: UsageCost {
            cache_read: 0.0315,
            cache_write: 0.0075,
            ..UsageCost::default()
        },
        timestamp: 120_000,
        ..Options::default()
    });
    let AgentMessage::Assistant(healthy) = &healthy else {
        panic!("expected assistant message");
    };
    assert!(detect_cache_miss(&entries, healthy, &Models).is_none());

    let AgentMessage::Assistant(first) = turn1() else {
        panic!("expected assistant message");
    };
    assert!(detect_cache_miss(&[], &first, &Models).is_none());
}
