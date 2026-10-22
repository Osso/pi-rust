//! Port of `packages/coding-agent/src/core/cache-stats.ts`.

use pi_ai::types::AssistantMessage;

use crate::core::session_manager::{KnownSessionEntry, SessionEntry};

/// Prompt-cache TTL: idle gaps longer than this suggest the likely cause of a miss.
pub const CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// Per-turn misses at or below this are cache breakpoint granularity noise.
const NOISE_FLOOR_TOKENS: f64 = 1024.0;

/// A counted cache miss on a single assistant message.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheMiss {
    /// Prompt tokens that were in the previous turn's prompt but not read from cache.
    pub missed_tokens: f64,
    /// Extra dollars paid vs. a full cache hit; 0 when pricing is unknown.
    pub missed_cost: f64,
    /// Milliseconds since the previous request (which last refreshed the cache).
    pub idle_ms: f64,
    /// True when the model changed relative to the previous request.
    pub model_changed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CacheWasteTotals {
    pub missed_tokens: f64,
    pub missed_cost: f64,
    pub miss_count: usize,
}

/// Minimal pricing lookup satisfied by the model runtime. Cost is $/million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    pub cache_read: f64,
}

pub trait ModelPriceSource {
    fn get_model(&self, provider: &str, model_id: &str) -> Option<ModelPrice>;
}

#[derive(Debug, Clone)]
struct PreviousRequest {
    prompt_tokens: f64,
    model_key: String,
    timestamp: i64,
    reported_cache: bool,
}

fn detect_miss(
    previous: Option<&PreviousRequest>,
    message: &AssistantMessage,
    models: &dyn ModelPriceSource,
) -> Option<CacheMiss> {
    let usage = &message.usage;
    let prompt_tokens = usage.input + usage.cache_read + usage.cache_write;
    let previous = previous?;
    if prompt_tokens <= 0.0 || (usage.cache_read + usage.cache_write == 0.0 && !previous.reported_cache) {
        return None;
    }

    let missed_tokens = previous.prompt_tokens.min(prompt_tokens) - usage.cache_read;
    if missed_tokens <= NOISE_FLOOR_TOKENS {
        return None;
    }

    let paid_tokens = usage.input + usage.cache_write;
    let paid_per_token = if paid_tokens > 0.0 {
        (usage.cost.input + usage.cost.cache_write) / paid_tokens
    } else {
        0.0
    };
    let read_per_token = if usage.cache_read > 0.0 {
        usage.cost.cache_read / usage.cache_read
    } else {
        models
            .get_model(&message.provider, &message.model)
            .map(|price| price.cache_read)
            .unwrap_or(0.0)
            / 1_000_000.0
    };

    Some(CacheMiss {
        missed_tokens,
        missed_cost: missed_tokens * (paid_per_token - read_per_token).max(0.0),
        idle_ms: (message.timestamp - previous.timestamp).max(0) as f64,
        model_changed: format!("{}/{}", message.provider, message.model) != previous.model_key,
    })
}

fn as_previous_request(message: &AssistantMessage, reported_cache: bool) -> Option<PreviousRequest> {
    let usage = &message.usage;
    let prompt_tokens = usage.input + usage.cache_read + usage.cache_write;
    if prompt_tokens <= 0.0 {
        return None;
    }
    Some(PreviousRequest {
        prompt_tokens,
        model_key: format!("{}/{}", message.provider, message.model),
        timestamp: message.timestamp,
        reported_cache: reported_cache || usage.cache_read + usage.cache_write > 0.0,
    })
}

struct Scan {
    previous: Option<PreviousRequest>,
    totals: CacheWasteTotals,
    /// Counted misses keyed by the entry index that paid for them.
    misses: Vec<(usize, CacheMiss)>,
}

fn scan(entries: &[SessionEntry], models: &dyn ModelPriceSource) -> Scan {
    let mut previous: Option<PreviousRequest> = None;
    let mut totals = CacheWasteTotals::default();
    let mut misses = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        match entry.known().as_deref() {
            Some(KnownSessionEntry::Compaction(_)) | Some(KnownSessionEntry::BranchSummary(_)) => {
                previous = None;
                continue;
            }
            Some(KnownSessionEntry::Message(message)) => {
                let assistant = match message.message.as_ref() {
                    pi_agent_core::agent::AgentMessage::Assistant(assistant) => assistant.as_ref(),
                    _ => continue,
                };
                if let Some(miss) = detect_miss(previous.as_ref(), assistant, models) {
                    totals.missed_tokens += miss.missed_tokens;
                    totals.missed_cost += miss.missed_cost;
                    totals.miss_count += 1;
                    misses.push((index, miss));
                }
                let reported_cache = previous
                    .as_ref()
                    .map(|previous| previous.reported_cache)
                    .unwrap_or(false);
                previous = as_previous_request(assistant, reported_cache).or(previous);
            }
            _ => {}
        }
    }

    Scan {
        previous,
        totals,
        misses,
    }
}

/// Cumulative cache waste across a session.
pub fn compute_cache_waste(entries: &[SessionEntry], models: &dyn ModelPriceSource) -> CacheWasteTotals {
    scan(entries, models).totals
}

/// All counted cache misses across a session, keyed by the entry index of the assistant message.
pub fn collect_cache_misses(entries: &[SessionEntry], models: &dyn ModelPriceSource) -> Vec<(usize, CacheMiss)> {
    scan(entries, models).misses
}

/// Detect a cache miss on a just-completed assistant message.
pub fn detect_cache_miss(
    entries: &[SessionEntry],
    message: &AssistantMessage,
    models: &dyn ModelPriceSource,
) -> Option<CacheMiss> {
    detect_miss(scan(entries, models).previous.as_ref(), message, models)
}
