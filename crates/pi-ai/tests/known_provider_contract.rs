//! Frozen types.ts KnownProvider literal union is the complete ordered public list.
use crate::shared_oracle as oracle;

#[test]
fn known_provider_literals_match_the_actual_frozen_export() {
    let mut source = oracle::TsOracle::spawn("known-provider-contract-oracle.mts", "packages/ai/src/types.ts");
    let expected: serde_json::Value = source.request(&serde_json::json!({}));
    assert_eq!(serde_json::json!(pi_ai::types::KNOWN_PROVIDERS.as_slice()), expected);
}
