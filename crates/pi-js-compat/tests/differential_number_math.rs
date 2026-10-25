use crate::shared_oracle_2 as oracle;
use pi_js_compat::number::{js_math_max, js_math_min};
use serde_json::json;
#[test]
fn shared_math_min_max_match_node_including_nan_and_signed_zero() {
    let values = [
        f64::NAN,
        f64::NEG_INFINITY,
        -f64::MAX,
        -1.5,
        -f64::MIN_POSITIVE,
        -0.0,
        0.0,
        f64::MIN_POSITIVE,
        1.5,
        f64::MAX,
        f64::INFINITY,
    ];
    let mut oracle = oracle::TsOracle::spawn("number-math-oracle.mjs", "packages/tui/src");
    for left in values {
        for right in values {
            let expected = oracle
                .request(&json!({"left":format!("{:016x}",left.to_bits()),"right":format!("{:016x}",right.to_bits())}));
            for (name, actual) in [("min", js_math_min(left, right)), ("max", js_math_max(left, right))] {
                let bits = u64::from_str_radix(expected[name].as_str().unwrap(), 16).unwrap();
                if actual.is_nan() {
                    assert!(f64::from_bits(bits).is_nan());
                } else {
                    assert_eq!(actual.to_bits(), bits, "{name}({left},{right})");
                }
            }
        }
    }
}
