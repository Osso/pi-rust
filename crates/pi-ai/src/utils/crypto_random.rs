//! Native entropy failure authority shared by OAuth and UUID generation.
//! Node v26.11.1 empty-queue error: docs/porting/node-crypto-random-source-lock.json.

use pi_js_compat::{js_json::JsValue, node_error::NativeNodeError};

pub(crate) fn fill_random(bytes: &mut [u8]) -> Result<(), NativeNodeError> {
    fill_random_with(bytes, getrandom::fill)
}

fn fill_random_with(
    bytes: &mut [u8],
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<(), NativeNodeError> {
    // getrandom has no OpenSSL error queue; use Node's exact empty-queue failure.
    fill(bytes).map_err(|_| {
        NativeNodeError::new("Error", "Deriving bits failed")
            .with_property("code", JsValue::String("ERR_CRYPTO_OPERATION_FAILED".into()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_failure_is_a_node_crypto_error_not_a_panic_or_fallback() {
        let mut bytes = [0; 32];
        let error = fill_random_with(&mut bytes, |_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(bytes, [0; 32]);
        assert_eq!(
            error.value(),
            pi_js_compat::js_json!({
                "name":"Error", "message":"Deriving bits failed", "code":"ERR_CRYPTO_OPERATION_FAILED"
            })
        );
    }
}
