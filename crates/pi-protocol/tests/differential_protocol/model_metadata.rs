use pi_protocol::{ModelCost, ModelInput, ModelMetadata, ModelMetadataInput, ThinkingLevel};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 3000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy)]
enum TaggedNumber {
    Finite(f64),
    NegativeZero,
    Nan,
    PositiveInfinity,
    NegativeInfinity,
}

struct Fixture {
    name: &'static str,
    provider: &'static str,
    id: &'static str,
    name_value: &'static str,
    api: &'static str,
    reasoning: bool,
    input: &'static [&'static str],
    context_window: u64,
    max_tokens: u64,
    cost_input: TaggedNumber,
    cost_output: TaggedNumber,
    cost_cache_read: TaggedNumber,
    cost_cache_write: TaggedNumber,
    supported_thinking_levels: &'static [&'static str],
    authenticated: bool,
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_model_metadata",
            "value": {
                "provider": self.provider,
                "id": self.id,
                "name": self.name_value,
                "api": self.api,
                "reasoning": self.reasoning,
                "input": self.input,
                "contextWindow": self.context_window,
                "maxTokens": self.max_tokens,
                "cost": {
                    "input": tagged_number_request(self.cost_input),
                    "output": tagged_number_request(self.cost_output),
                    "cacheRead": tagged_number_request(self.cost_cache_read),
                    "cacheWrite": tagged_number_request(self.cost_cache_write),
                },
                "supportedThinkingLevels": self.supported_thinking_levels,
                "authenticated": self.authenticated,
            },
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        match self.construct_metadata() {
            Some(metadata) => valid_response(id, &metadata),
            None => invalid_response(id),
        }
    }

    fn construct_metadata(&self) -> Option<ModelMetadata> {
        let input = parse_model_inputs(self.input)?;
        let supported_thinking_levels = parse_thinking_levels(self.supported_thinking_levels)?;
        let cost = self.construct_cost()?;
        ModelMetadata::new(ModelMetadataInput {
            provider: self.provider.to_owned(),
            id: self.id.to_owned(),
            name: self.name_value.to_owned(),
            api: self.api.to_owned(),
            reasoning: self.reasoning,
            input,
            context_window: self.context_window,
            max_tokens: self.max_tokens,
            cost,
            supported_thinking_levels,
            authenticated: self.authenticated,
        })
        .ok()
    }

    fn construct_cost(&self) -> Option<ModelCost> {
        ModelCost::new(
            decode_tagged_number(self.cost_input),
            decode_tagged_number(self.cost_output),
            decode_tagged_number(self.cost_cache_read),
            decode_tagged_number(self.cost_cache_write),
        )
        .ok()
    }
}

fn parse_model_inputs(values: &[&str]) -> Option<Vec<ModelInput>> {
    values.iter().map(|value| value.parse().ok()).collect()
}

fn parse_thinking_levels(values: &[&str]) -> Option<Vec<ThinkingLevel>> {
    values.iter().map(|value| value.parse().ok()).collect()
}

fn tagged_number_request(number: TaggedNumber) -> Value {
    match number {
        TaggedNumber::Finite(value) => json!({"number": "finite", "value": value}),
        TaggedNumber::NegativeZero => json!({"number": "negative_zero"}),
        TaggedNumber::Nan => json!({"number": "nan"}),
        TaggedNumber::PositiveInfinity => json!({"number": "positive_infinity"}),
        TaggedNumber::NegativeInfinity => json!({"number": "negative_infinity"}),
    }
}

fn decode_tagged_number(number: TaggedNumber) -> f64 {
    match number {
        TaggedNumber::Finite(value) => value,
        TaggedNumber::NegativeZero => -0.0,
        TaggedNumber::Nan => f64::NAN,
        TaggedNumber::PositiveInfinity => f64::INFINITY,
        TaggedNumber::NegativeInfinity => f64::NEG_INFINITY,
    }
}

fn float64_bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn valid_response(id: u64, metadata: &ModelMetadata) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": normalized_metadata(metadata),
        },
    })
}

fn normalized_metadata(metadata: &ModelMetadata) -> Value {
    let input = metadata.input().iter().map(|value| value.as_str()).collect::<Vec<_>>();
    let supported_thinking_levels = metadata
        .supported_thinking_levels()
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>();

    json!({
        "provider": metadata.provider(),
        "id": metadata.id(),
        "name": metadata.name(),
        "api": metadata.api(),
        "reasoning": metadata.reasoning(),
        "input": input,
        "contextWindow": metadata.context_window(),
        "maxTokens": metadata.max_tokens(),
        "cost": normalized_cost(metadata.cost()),
        "supportedThinkingLevels": supported_thinking_levels,
        "authenticated": metadata.authenticated(),
    })
}

fn normalized_cost(cost: ModelCost) -> Value {
    json!({
        "input": float64_bits(cost.input()),
        "output": float64_bits(cost.output()),
        "cacheRead": float64_bits(cost.cache_read()),
        "cacheWrite": float64_bits(cost.cache_write()),
    })
}

fn invalid_response(id: u64) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": false},
    })
}

fn ordinary_fixture() -> Fixture {
    Fixture {
        name: "ordinary baseline",
        provider: "openai",
        id: "gpt-5",
        name_value: "GPT-5",
        api: "openai-responses",
        reasoning: true,
        input: &["text"],
        context_window: 128_000,
        max_tokens: 4_096,
        cost_input: TaggedNumber::Finite(1.0),
        cost_output: TaggedNumber::Finite(2.0),
        cost_cache_read: TaggedNumber::Finite(0.0),
        cost_cache_write: TaggedNumber::Finite(0.0),
        supported_thinking_levels: &["high"],
        authenticated: true,
    }
}

fn configured_fixture(name: &'static str, configure: impl FnOnce(&mut Fixture)) -> Fixture {
    let mut fixture = ordinary_fixture();
    fixture.name = name;
    configure(&mut fixture);
    fixture
}

fn one_character_fixture() -> Fixture {
    configured_fixture("one-character strings", |fixture| {
        fixture.provider = "p";
        fixture.id = "i";
        fixture.name_value = "n";
        fixture.api = "a";
    })
}

fn unicode_fixture() -> Fixture {
    configured_fixture("Unicode, whitespace, combining, and NUL strings", |fixture| {
        fixture.provider = " \t";
        fixture.id = "𐅑";
        fixture.name_value = "e\u{301}";
        fixture.api = "api\0name";
    })
}

fn fractional_cost_fixture() -> Fixture {
    configured_fixture("fractional and negative-zero costs", |fixture| {
        fixture.cost_input = TaggedNumber::Finite(110.5);
        fixture.cost_output = TaggedNumber::Finite(0.25);
        fixture.cost_cache_read = TaggedNumber::NegativeZero;
        fixture.cost_cache_write = TaggedNumber::Finite(2.5);
        fixture.supported_thinking_levels = &["off", "max", "off"];
    })
}

fn all_levels_fixture() -> Fixture {
    configured_fixture("all thinking levels", |fixture| {
        fixture.supported_thinking_levels = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
    })
}

fn valid_fixtures() -> Vec<Fixture> {
    vec![
        ordinary_fixture(),
        one_character_fixture(),
        unicode_fixture(),
        configured_fixture("empty input", |fixture| fixture.input = &[]),
        configured_fixture("ordered duplicate inputs", |fixture| {
            fixture.input = &["image", "text", "text"];
        }),
        configured_fixture("both booleans false", |fixture| {
            fixture.reasoning = false;
            fixture.authenticated = false;
        }),
        configured_fixture("count boundaries", |fixture| {
            fixture.context_window = 1;
            fixture.max_tokens = MAX_SAFE_INTEGER;
        }),
        fractional_cost_fixture(),
        all_levels_fixture(),
    ]
}

fn nonfinite_cost_fixture() -> Fixture {
    configured_fixture("nonfinite cost", |fixture| {
        fixture.cost_output = TaggedNumber::Nan;
        fixture.cost_cache_read = TaggedNumber::NegativeInfinity;
        fixture.cost_cache_write = TaggedNumber::PositiveInfinity;
    })
}

fn invalid_fixtures() -> Vec<Fixture> {
    vec![
        configured_fixture("empty provider", |fixture| fixture.provider = ""),
        configured_fixture("empty id", |fixture| fixture.id = ""),
        configured_fixture("empty name", |fixture| fixture.name_value = ""),
        configured_fixture("empty api", |fixture| fixture.api = ""),
        configured_fixture("unsupported input", |fixture| fixture.input = &["audio"]),
        configured_fixture("zero context window", |fixture| fixture.context_window = 0),
        configured_fixture("zero max tokens", |fixture| fixture.max_tokens = 0),
        configured_fixture("negative cost", |fixture| {
            fixture.cost_input = TaggedNumber::Finite(-1.0);
        }),
        nonfinite_cost_fixture(),
        configured_fixture("empty thinking levels", |fixture| {
            fixture.supported_thinking_levels = &[];
        }),
        configured_fixture("unsupported ultra thinking level", |fixture| {
            fixture.supported_thinking_levels = &["ultra"];
        }),
        configured_fixture("mixed valid and ultra thinking levels", |fixture| {
            fixture.supported_thinking_levels = &["high", "ultra"];
        }),
    ]
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let mut id = FIRST_REQUEST_ID;
    for fixture in valid_fixtures().iter().chain(invalid_fixtures().iter()) {
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
        id += 1;
    }
    assert_eq!(id, FIRST_REQUEST_ID + 21);
}
