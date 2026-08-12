use pi_protocol::{Usage, UsageCost, UsageInput};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 4000;
const FIXTURE_COUNT: u64 = 14;
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
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    reasoning: Option<u64>,
    total_tokens: u64,
    cost_input: TaggedNumber,
    cost_output: TaggedNumber,
    cost_cache_read: TaggedNumber,
    cost_cache_write: TaggedNumber,
    cost_total: TaggedNumber,
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        let mut value = json!({
            "input": self.input,
            "output": self.output,
            "cacheRead": self.cache_read,
            "cacheWrite": self.cache_write,
            "totalTokens": self.total_tokens,
            "cost": {
                "input": tagged_number_request(self.cost_input),
                "output": tagged_number_request(self.cost_output),
                "cacheRead": tagged_number_request(self.cost_cache_read),
                "cacheWrite": tagged_number_request(self.cost_cache_write),
                "total": tagged_number_request(self.cost_total),
            },
        });
        if let Some(reasoning) = self.reasoning {
            value["reasoning"] = json!(reasoning);
        }
        json!({
            "id": id,
            "operation": "validate_usage",
            "value": value,
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        let cost = match UsageCost::new(
            decode_tagged_number(self.cost_input),
            decode_tagged_number(self.cost_output),
            decode_tagged_number(self.cost_cache_read),
            decode_tagged_number(self.cost_cache_write),
            decode_tagged_number(self.cost_total),
        ) {
            Ok(cost) => cost,
            Err(_) => return invalid_response(id),
        };
        let usage = Usage::new(UsageInput {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            reasoning: self.reasoning,
            total_tokens: self.total_tokens,
            cost,
        });
        valid_response(id, usage)
    }
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

fn valid_response(id: u64, usage: Usage) -> Value {
    let mut value = json!({
        "input": usage.input(),
        "output": usage.output(),
        "cacheRead": usage.cache_read(),
        "cacheWrite": usage.cache_write(),
        "totalTokens": usage.total_tokens(),
        "cost": {
            "input": float64_bits(usage.cost().input()),
            "output": float64_bits(usage.cost().output()),
            "cacheRead": float64_bits(usage.cost().cache_read()),
            "cacheWrite": float64_bits(usage.cost().cache_write()),
            "total": float64_bits(usage.cost().total()),
        },
    });
    if let Some(reasoning) = usage.reasoning() {
        value["reasoning"] = json!(reasoning);
    }
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": value,
        },
    })
}

fn invalid_response(id: u64) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": false},
    })
}

fn baseline() -> Fixture {
    Fixture {
        name: "baseline with omitted reasoning and independent total",
        input: 1000,
        output: 2000,
        cache_read: 3000,
        cache_write: 4000,
        reasoning: None,
        total_tokens: 7,
        cost_input: TaggedNumber::Finite(1.0),
        cost_output: TaggedNumber::Finite(2.0),
        cost_cache_read: TaggedNumber::Finite(3.0),
        cost_cache_write: TaggedNumber::Finite(4.0),
        cost_total: TaggedNumber::Finite(5.0),
    }
}

fn configured_fixture(name: &'static str, configure: impl FnOnce(&mut Fixture)) -> Fixture {
    let mut fixture = baseline();
    fixture.name = name;
    configure(&mut fixture);
    fixture
}

fn zero_tokens_fixture() -> Fixture {
    configured_fixture("all token fields zero", |fixture| {
        fixture.input = 0;
        fixture.output = 0;
        fixture.cache_read = 0;
        fixture.cache_write = 0;
        fixture.reasoning = Some(0);
        fixture.total_tokens = 0;
    })
}

fn maximum_tokens_fixture() -> Fixture {
    configured_fixture("all token fields maximum safe integer", |fixture| {
        fixture.input = MAX_SAFE_INTEGER;
        fixture.output = MAX_SAFE_INTEGER;
        fixture.cache_read = MAX_SAFE_INTEGER;
        fixture.cache_write = MAX_SAFE_INTEGER;
        fixture.reasoning = Some(MAX_SAFE_INTEGER);
        fixture.total_tokens = MAX_SAFE_INTEGER;
    })
}

fn fractional_cost_fixture() -> Fixture {
    configured_fixture("fractional and negative-zero costs", |fixture| {
        fixture.cost_input = TaggedNumber::Finite(110.5);
        fixture.cost_output = TaggedNumber::Finite(0.25);
        fixture.cost_cache_read = TaggedNumber::NegativeZero;
        fixture.cost_cache_write = TaggedNumber::Finite(2.5);
        fixture.cost_total = TaggedNumber::Finite(3.75);
    })
}

fn extreme_cost_fixture() -> Fixture {
    configured_fixture("maximum and subnormal finite costs", |fixture| {
        fixture.cost_input = TaggedNumber::Finite(f64::MAX);
        fixture.cost_output = TaggedNumber::Finite(f64::MIN_POSITIVE);
        fixture.cost_cache_read = TaggedNumber::Finite(f64::from_bits(1));
        fixture.cost_cache_write = TaggedNumber::Finite(f64::MAX);
        fixture.cost_total = TaggedNumber::Finite(f64::MIN_POSITIVE);
    })
}

fn valid_fixtures() -> Vec<Fixture> {
    vec![
        baseline(),
        configured_fixture("explicit zero reasoning", |fixture| fixture.reasoning = Some(0)),
        zero_tokens_fixture(),
        maximum_tokens_fixture(),
        fractional_cost_fixture(),
        extreme_cost_fixture(),
    ]
}

fn invalid_fixtures() -> Vec<Fixture> {
    vec![
        configured_fixture("negative input cost", |fixture| {
            fixture.cost_input = TaggedNumber::Finite(-1.0);
        }),
        configured_fixture("negative output cost", |fixture| {
            fixture.cost_output = TaggedNumber::Finite(-1.0);
        }),
        configured_fixture("negative cache-read cost", |fixture| {
            fixture.cost_cache_read = TaggedNumber::Finite(-1.0);
        }),
        configured_fixture("negative cache-write cost", |fixture| {
            fixture.cost_cache_write = TaggedNumber::Finite(-1.0);
        }),
        configured_fixture("negative total cost", |fixture| {
            fixture.cost_total = TaggedNumber::Finite(-1.0);
        }),
        configured_fixture("NaN input cost", |fixture| {
            fixture.cost_input = TaggedNumber::Nan;
        }),
        configured_fixture("positive-infinity output cost", |fixture| {
            fixture.cost_output = TaggedNumber::PositiveInfinity;
        }),
        configured_fixture("negative-infinity total cost", |fixture| {
            fixture.cost_total = TaggedNumber::NegativeInfinity;
        }),
    ]
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = valid_fixtures().into_iter().chain(invalid_fixtures());
    let mut next_id = FIRST_REQUEST_ID;
    for fixture in fixtures {
        let expected = oracle.request(&fixture.request(next_id));
        let actual = fixture.rust_response(next_id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
        next_id += 1;
    }
    assert_eq!(next_id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}
