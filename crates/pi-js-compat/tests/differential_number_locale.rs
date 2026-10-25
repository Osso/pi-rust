use crate::shared_oracle_2 as oracle;

use pi_js_compat::number::to_locale_string;
use serde_json::json;

#[test]
fn default_en_us_numbers_match_pinned_node() {
    let mut values = vec![
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        1234.0,
        1234567.0,
        -1234.0,
        12345.25,
        -12345.25,
        999.9999,
        1.0005,
        -1.0005,
        1e21,
        1e-7,
    ];
    for value in [0.0005_f64, 0.9995, 1.2345, 12.9995, 999.9995, 1000000000000000100.0] {
        values.extend([
            f64::from_bits(value.to_bits() - 1),
            value,
            f64::from_bits(value.to_bits() + 1),
            -value,
        ]);
    }
    let mut seed = 0x123456789abcdef0_u64;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.push(f64::from_bits(seed));
    }
    let mut oracle = oracle::TsOracle::spawn(
        "number-locale-oracle.mjs",
        "packages/coding-agent/src/modes/interactive/interactive-mode.ts",
    );
    let request = json!({"bits":values.iter().map(|value|format!("{:016x}",value.to_bits())).collect::<Vec<_>>()});
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "source: {expected}");
    for (index, value) in values.into_iter().enumerate() {
        assert_eq!(
            json!(to_locale_string(value)),
            expected["output"][index],
            "bits {:016x}",
            value.to_bits()
        );
    }
}
