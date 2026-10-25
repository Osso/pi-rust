use crate::shared_oracle_2 as oracle;

use pi_js_compat::number::parse_float_utf16;
use serde_json::json;

#[test]
fn utf16_parse_float_matches_node_bits_at_every_numeric_prefix_boundary() {
    let strings = [
        "",
        "0",
        "-0",
        "+",
        "-",
        ".",
        "1.",
        ".5",
        "-12.5e-3",
        "1e",
        "1e+",
        "1e-",
        "1e309",
        "1e-400",
        "Infinity",
        "+Infinity",
        "-Infinity",
        "NaN",
        "inf",
        "0x10",
        "12 34",
    ];
    let separators = [
        0xd800, 0xdfff, 0x2000, 0xa0, 0xfeff, 0x85, 0x2028, 0x3000, 0xfffe, 0x660, 0x0, 0x61,
    ];
    let mut source = oracle::TsOracle::spawn("parse-float-utf16-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for text in strings {
        let plain = text.encode_utf16().collect::<Vec<_>>();
        for separator in separators {
            for index in 0..=plain.len() {
                let mut units = plain.clone();
                units.insert(index, separator);
                let expected = source.request(&json!({"units":units}));
                let value = parse_float_utf16(&units);
                let actual = if value.is_nan() {
                    "NaN".into()
                } else {
                    format!("{:016x}", value.to_bits())
                };
                assert_eq!(expected["bits"], actual, "units={units:?}");
            }
        }
    }
}
