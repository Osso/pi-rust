//! Native crypto.randomUUID binding used by Codex and coding-agent boundaries.

use pi_js_compat::node_error::NativeNodeError;

/// `crypto.randomUUID()`: RFC 9562 version 4 from OS randomness.
pub fn random_uuid() -> Result<String, NativeNodeError> {
    random_uuid_with(crate::utils::crypto_random::fill_random)
}

fn random_uuid_with(fill: impl FnOnce(&mut [u8]) -> Result<(), NativeNodeError>) -> Result<String, NativeNodeError> {
    let mut bytes = [0_u8; 16];
    fill(&mut bytes)?;
    bytes[6] = 0x40 | (bytes[6] & 0x0f);
    bytes[8] = 0x80 | (bytes[8] & 0x3f);
    Ok(format_uuid(&bytes))
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_entropy_failure_preserves_original_error_and_does_not_retry() {
        let expected = NativeNodeError::new("Error", "Deriving bits failed")
            .with_property("code", pi_js_compat::js_json!("ERR_CRYPTO_OPERATION_FAILED"));
        let mut calls = 0;
        let error = random_uuid_with(|bytes| {
            assert_eq!(bytes.len(), 16);
            calls += 1;
            Err(expected.clone())
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error, expected);
    }

    #[test]
    fn uuid_formats_exact_entropy_and_sets_only_rfc_version_and_variant_bits() {
        for (byte, expected) in [
            (0, "00000000-0000-4000-8000-000000000000"),
            (255, "ffffffff-ffff-4fff-bfff-ffffffffffff"),
        ] {
            let actual = random_uuid_with(|bytes| {
                bytes.fill(byte);
                Ok(())
            })
            .unwrap();
            assert_eq!(actual, expected);
        }
    }
}
