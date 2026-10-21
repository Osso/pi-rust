//! Narrow artifact-deletion coupling: actual builtinProviders IDs/order only.
//! Does not establish whole all.ts/factory/protocol/auth parity.
use pi_ai::providers::all::builtin_providers;
use serde_json::json;

#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn builtin_factory_ids_and_order_match_frozen_all_module() {
    let mut ts = oracle::TsOracle::spawn("provider-inventory-oracle.ts", "packages/ai/src/providers/all.ts");
    let response = ts.request(&json!({"id": 1}));
    assert_eq!(response["ok"], true);
    let providers = builtin_providers();
    let ids = providers.iter().map(|provider| provider.id()).collect::<Vec<_>>();
    assert_eq!(json!(ids), response["output"]);
}
