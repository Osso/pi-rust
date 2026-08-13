use pi_agent_core::harness::{
    BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX, COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX,
};
use serde_json::json;

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_096;

pub(crate) fn compare(oracle: &mut TsOracle) {
    let request = json!({
        "id": REQUEST_ID,
        "operation": "summary_markers",
    });
    let expected = oracle.request(&request);
    let actual = ok_response(
        REQUEST_ID,
        json!({
            "compactionPrefix": COMPACTION_SUMMARY_PREFIX,
            "compactionSuffix": COMPACTION_SUMMARY_SUFFIX,
            "branchPrefix": BRANCH_SUMMARY_PREFIX,
            "branchSuffix": BRANCH_SUMMARY_SUFFIX,
        }),
    );
    assert_eq!(actual, expected, "summary marker mismatch");
}
