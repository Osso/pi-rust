use crate::shared_oracle as oracle;
use pi_coding_agent::core::footer_data_provider::FooterDataProvider;
use pi_js_compat::{js_json, js_json::JsValue};
const SOURCE: &str = "packages/coding-agent/src/core/footer-data-provider.ts";

#[tokio::test]
async fn provider_count_preserves_the_full_declared_number_domain() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("footer-provider-oracle.mts", SOURCE);
    let expected = source.request(&js_json!({"operation":"numbers","cwd":cwd}));
    let provider = FooterDataProvider::new(cwd);
    let mut actual = Vec::new();
    for number in [
        0.0,
        -1.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        9_007_199_254_740_992.0,
        -0.0,
    ] {
        provider.set_available_provider_count(number);
        let value = provider.get_available_provider_count();
        actual.push(js_json!({"value":value,"negativeZero":value == 0.0 && value.is_sign_negative()}));
    }
    provider.dispose();
    assert_eq!(JsValue::Array(actual), expected);
}
