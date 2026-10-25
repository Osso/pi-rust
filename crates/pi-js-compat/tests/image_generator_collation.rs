use crate::shared_oracle_2 as oracle;
use pi_js_compat::{
    collator::locale_compare_utf16,
    js_json::{JsValue, parse_lossless},
};
use serde_json::json;
#[test]
fn lone_surrogate_collation_matches_node_pair_order() {
    let mut ts = oracle::TsOracle::spawn(
        "image-generator-coercion-oracle.ts",
        "packages/ai/scripts/generate-image-models.ts",
    );
    let values = [
        r#""\ud800""#,
        r#""\ud801""#,
        r#""\udc00""#,
        r#""�""#,
        r#""a""#,
        r#""A""#,
        r#""10""#,
        r#""2""#,
        r#""😀""#,
        r#""x\ud800a""#,
        r#""\ud800á""#,
        r#""á\ud800""#,
        r#""\ud800\udc00""#,
        r#""\ud800\ud800""#,
        r#""é""#,
        r#""é""#,
    ];
    for left in values {
        for right in values {
            let JsValue::String(a) = parse_lossless(left).unwrap() else {
                unreachable!()
            };
            let JsValue::String(b) = parse_lossless(right).unwrap() else {
                unreachable!()
            };
            let actual = locale_compare_utf16(a.units(), b.units()) as i32;
            assert_eq!(
                json!({"order":actual}),
                ts.request(&json!({"operation":"compare","left":left,"right":right})),
                "{left} vs {right}"
            );
        }
    }
}
