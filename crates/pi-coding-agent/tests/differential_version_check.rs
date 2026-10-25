//! Actual frozen semver comparison/newer proof. Release transport is in version_check_release.rs.
use crate::shared_oracle as oracle;

use pi_coding_agent::utils::version_check::{compare_package_versions, is_newer_package_version};
use pi_js_compat::js_json::JsString;
use serde_json::json;

const VERSIONS: [&str; 26] = [
    "0.70.6",
    "0.70.5",
    "0.70.4",
    "5.0.0-beta.20",
    "5.0.0-beta.9",
    "1.2.3",
    "v1.2.3",
    " 1.2.3 ",
    "1.2",
    "1.2.3.4",
    "01.2.3",
    "1.2.3-01",
    "1.2.3-0",
    "1.2.3-alpha.1",
    "1.2.3-alpha.beta",
    "1.2.3+build.5",
    "1.2.3-rc.1+build",
    "garbage",
    "",
    "1.2.3-",
    "1.2.3+",
    "99999999999999999999.0.0",
    "1.0.0-alpha",
    "1.0.0-alpha.1",
    "1.0.0-beta",
    "1.0.0",
];

#[test]
fn semver_comparison_matches_node_semver() {
    let mut oracle = oracle::TsOracle::spawn(
        "version-check-oracle.ts",
        "packages/coding-agent/src/utils/version-check.ts",
    );
    let mut id = 0;
    for left in VERSIONS {
        for right in VERSIONS {
            id += 1;
            let response = oracle.request(&json!({"id": id, "op": "compare", "left": left, "right": right}));
            assert_eq!(response["ok"], true, "{response}");
            let comparison =
                compare_package_versions(&JsString::from(left), &JsString::from(right)).map(|value| value.signum());
            assert_eq!(
                json!(comparison),
                response["output"]["comparison"],
                "compare {left:?} {right:?}"
            );
            id += 1;
            let response = oracle.request(&json!({"id": id, "op": "newer", "candidate": left, "current": right}));
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(
                json!(is_newer_package_version(&JsString::from(left), &JsString::from(right))),
                response["output"]["newer"],
                "newer {left:?} {right:?}"
            );
        }
    }
}
