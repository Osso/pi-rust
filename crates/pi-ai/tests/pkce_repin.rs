use crate::shared_oracle as oracle;

use pi_ai::auth::oauth::pkce::generate_pkce_with;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

#[test]
fn generated_verifier_and_challenge_match_actual_frozen_crypto_inputs() {
    // Frozen utils/oauth/pkce.ts:21-33, unchanged source with injected Web Crypto.
    let mut source = oracle::TsOracle::spawn("pkce-repin-oracle.mts", "packages/ai/src/utils/oauth/pkce.ts");
    for bytes in [
        vec![0; 32],
        vec![255; 32],
        (0..32).collect(),
        (0..32).map(|byte| byte * 7).collect(),
    ] {
        let expected = source.request(&js_json!({"bytes":bytes}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let mut random_calls = 0;
        let pkce = generate_pkce_with(|buffer| {
            assert_eq!(buffer.len(), 32);
            random_calls += 1;
            buffer.copy_from_slice(&bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(random_calls, 1);
        assert_eq!(
            js_json!({"verifier":pkce.verifier,"challenge":pkce.challenge}),
            expected["value"]
        );
        assert_eq!(
            expected["calls"],
            js_json!([{"operation":"random","length":32},{"operation":"digest","algorithm":"SHA-256","bytes":pkce.verifier.as_bytes()}])
        );
    }
}

struct RejectedRandom(pi_ai::request::CallbackError);
impl pi_ai::auth::oauth::support::RandomSource for RejectedRandom {
    fn fill(&self, _: &mut [u8]) -> Result<(), pi_ai::request::CallbackError> {
        Err(self.0.clone())
    }
    fn uuid(&self) -> Result<String, pi_ai::request::CallbackError> {
        pi_ai::auth::oauth::support::uuid_v4(self)
    }
}

#[test]
fn random_source_rejection_preserves_original_callback_error() {
    for reason in [
        pi_js_compat::abort::AbortReason::Value(JsValue::String(JsString::from_utf16(vec![0xd800]))),
        pi_js_compat::node_error::NativeNodeError::new("Error", "Deriving bits failed")
            .with_property("code", JsValue::String("ERR_CRYPTO_OPERATION_FAILED".into()))
            .into(),
    ] {
        let expected = pi_ai::request::CallbackError::from_rejection(reason);
        let random = RejectedRandom(expected.clone());
        for actual in [
            generate_pkce_with(|_| Err(expected.clone())).unwrap_err(),
            pi_ai::auth::oauth::support::random_hex(&random, 16).unwrap_err(),
            pi_ai::auth::oauth::support::uuid_v4(&random).unwrap_err(),
        ] {
            assert_eq!(actual.thrown_value(), expected.thrown_value());
        }
    }
}

#[test]
fn frozen_crypto_failures_reject_exact_values_without_fallback() {
    let mut source = oracle::TsOracle::spawn("pkce-repin-oracle.mts", "packages/ai/src/utils/oauth/pkce.ts");
    for stage in ["random", "digest"] {
        for raw in [false, true] {
            let expected = source.request(&js_json!({"bytes":vec![0;32],"reject":stage,"raw":raw}));
            assert_eq!(expected["ok"], JsValue::Bool(false), "{expected:?}");
            let value = if raw {
                JsValue::String(JsString::from("pkce-") + JsString::from_utf16(vec![0xd800]))
            } else {
                js_json!({"name":"TypeError","message":JsString::from("crypto-")+JsString::from_utf16(vec![0xdfff]),"code":"E_CRYPTO"})
            };
            assert_eq!(expected["value"], value);
            assert_eq!(
                expected["calls"].as_array().unwrap().len(),
                if stage == "random" { 1 } else { 2 }
            );
        }
    }
}
