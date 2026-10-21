use pi_agent_core::BeforeToolCallResult;

#[test]
fn preserves_independent_optional_block_and_reason_values() {
    for block in [None, Some(false), Some(true)] {
        for reason in [None, Some(""), Some("blocked\n理由🙂\0next")] {
            let result = BeforeToolCallResult {
                block,
                reason: reason.map(str::to_owned),
            };
            assert_eq!(result.block, block);
            assert_eq!(result.reason.as_deref(), reason);
        }
    }
}

#[test]
fn hook_result_fields_remain_mutable() {
    let mut result = BeforeToolCallResult {
        block: None,
        reason: None,
    };
    result.block = Some(true);
    result.reason = Some("changed".into());
    assert_eq!(result.block, Some(true));
    assert_eq!(result.reason.as_deref(), Some("changed"));
}
