use pi_protocol::{ClientHello, PROTOCOL_VERSION};
use serde_json::json;

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 8022;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let versions = [0, PROTOCOL_VERSION, 2, MAX_SAFE_INTEGER];

    for (offset, version) in versions.into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let request = json!({
            "id": id,
            "operation": "validate_client_hello",
            "value": {"type": "hello", "version": version},
        });
        let expected = oracle.request(&request);
        let hello = ClientHello::new(version);
        let actual = json!({
            "id": id,
            "ok": true,
            "output": {
                "valid": true,
                "value": {"type": "hello", "version": hello.version()},
            },
        });

        assert_eq!(actual, expected, "client hello mismatch for version {version}");
    }
}
