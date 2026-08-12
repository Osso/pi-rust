use pi_protocol::ModelCost;
use serde_json::{Value, json};

use super::oracle::TsOracle;

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
    input: TaggedNumber,
    output: TaggedNumber,
    cache_read: TaggedNumber,
    cache_write: TaggedNumber,
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_model_cost",
            "value": {
                "input": tagged_number_request(self.input),
                "output": tagged_number_request(self.output),
                "cacheRead": tagged_number_request(self.cache_read),
                "cacheWrite": tagged_number_request(self.cache_write),
            },
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        let values = [
            decode_tagged_number(self.input),
            decode_tagged_number(self.output),
            decode_tagged_number(self.cache_read),
            decode_tagged_number(self.cache_write),
        ];
        match ModelCost::new(values[0], values[1], values[2], values[3]) {
            Ok(cost) => json!({
                "id": id,
                "ok": true,
                "output": {
                    "valid": true,
                    "bits": {
                        "input": float64_bits(cost.input()),
                        "output": float64_bits(cost.output()),
                        "cacheRead": float64_bits(cost.cache_read()),
                        "cacheWrite": float64_bits(cost.cache_write()),
                    },
                },
            }),
            Err(_) => json!({
                "id": id,
                "ok": true,
                "output": {"valid": false},
            }),
        }
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

fn valid_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "integer costs",
            input: TaggedNumber::Finite(1000.0),
            output: TaggedNumber::Finite(1000.0),
            cache_read: TaggedNumber::Finite(1000.0),
            cache_write: TaggedNumber::Finite(1000.0),
        },
        Fixture {
            name: "fractional costs",
            input: TaggedNumber::Finite(0.2),
            output: TaggedNumber::Finite(110.5),
            cache_read: TaggedNumber::Finite(0.02),
            cache_write: TaggedNumber::Finite(2.5),
        },
        Fixture {
            name: "zero negative zero subnormal and max",
            input: TaggedNumber::Finite(0.0),
            output: TaggedNumber::NegativeZero,
            cache_read: TaggedNumber::Finite(f64::from_bits(1)),
            cache_write: TaggedNumber::Finite(f64::MAX),
        },
    ]
}

fn negative_fixtures() -> Vec<Fixture> {
    let zero = TaggedNumber::Finite(0.0);
    vec![
        Fixture {
            name: "negative input",
            input: TaggedNumber::Finite(-1.0),
            output: zero,
            cache_read: zero,
            cache_write: zero,
        },
        Fixture {
            name: "negative output",
            input: zero,
            output: TaggedNumber::Finite(-1.0),
            cache_read: zero,
            cache_write: zero,
        },
        Fixture {
            name: "negative cache read",
            input: zero,
            output: zero,
            cache_read: TaggedNumber::Finite(-f64::MIN_POSITIVE),
            cache_write: zero,
        },
        Fixture {
            name: "negative cache write",
            input: zero,
            output: zero,
            cache_read: zero,
            cache_write: TaggedNumber::Finite(-1.0),
        },
    ]
}

fn non_finite_fixtures() -> Vec<Fixture> {
    let zero = TaggedNumber::Finite(0.0);
    vec![
        Fixture {
            name: "nan input",
            input: TaggedNumber::Nan,
            output: zero,
            cache_read: zero,
            cache_write: zero,
        },
        Fixture {
            name: "positive infinity output",
            input: zero,
            output: TaggedNumber::PositiveInfinity,
            cache_read: zero,
            cache_write: zero,
        },
        Fixture {
            name: "negative infinity cache read",
            input: zero,
            output: zero,
            cache_read: TaggedNumber::NegativeInfinity,
            cache_write: zero,
        },
        Fixture {
            name: "nan cache write",
            input: zero,
            output: zero,
            cache_read: zero,
            cache_write: TaggedNumber::Nan,
        },
    ]
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = valid_fixtures();
    fixtures.extend(negative_fixtures());
    fixtures.extend(non_finite_fixtures());
    fixtures
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle, first_id: u64) {
    for (index, fixture) in fixtures().iter().enumerate() {
        let id = first_id + u64::try_from(index).expect("fixture index must fit in request id");
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
    }
}
