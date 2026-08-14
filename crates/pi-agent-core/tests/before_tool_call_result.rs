use std::fmt::Debug;

use pi_agent_core::harness::BeforeToolCallResult;
use pi_agent_core::harness::types::BeforeToolCallResult as ModuleBeforeToolCallResult;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn result_is_public_from_module_and_harness_root() {
    let module_result = ModuleBeforeToolCallResult {
        block: Some(true),
        reason: Some("module".to_owned()),
        terminate: Some(false),
    };
    let root_result = BeforeToolCallResult {
        block: Some(false),
        reason: Some("root".to_owned()),
        terminate: Some(true),
    };

    assert_eq!(module_result.block, Some(true));
    assert_eq!(module_result.reason.as_deref(), Some("module"));
    assert_eq!(module_result.terminate, Some(false));
    assert_eq!(root_result.block, Some(false));
    assert_eq!(root_result.reason.as_deref(), Some("root"));
    assert_eq!(root_result.terminate, Some(true));
    assert_debug(&module_result);
    assert_debug(&root_result);
}

#[test]
fn result_preserves_all_omitted_fields() {
    let result = BeforeToolCallResult {
        block: None,
        reason: None,
        terminate: None,
    };

    assert!(result.block.is_none());
    assert!(result.reason.is_none());
    assert!(result.terminate.is_none());
}

#[test]
fn result_preserves_independent_optional_field_states() {
    let results = [
        BeforeToolCallResult {
            block: Some(false),
            reason: None,
            terminate: None,
        },
        BeforeToolCallResult {
            block: Some(true),
            reason: None,
            terminate: None,
        },
        BeforeToolCallResult {
            block: None,
            reason: Some(String::new()),
            terminate: None,
        },
        BeforeToolCallResult {
            block: None,
            reason: None,
            terminate: Some(false),
        },
        BeforeToolCallResult {
            block: None,
            reason: None,
            terminate: Some(true),
        },
    ];

    assert_eq!(
        (results[0].block, results[0].reason.as_deref(), results[0].terminate,),
        (Some(false), None, None),
    );
    assert_eq!(
        (results[1].block, results[1].reason.as_deref(), results[1].terminate,),
        (Some(true), None, None),
    );
    assert_eq!(
        (results[2].block, results[2].reason.as_deref(), results[2].terminate,),
        (None, Some(""), None),
    );
    assert_eq!(
        (results[3].block, results[3].reason.as_deref(), results[3].terminate,),
        (None, None, Some(false)),
    );
    assert_eq!(
        (results[4].block, results[4].reason.as_deref(), results[4].terminate,),
        (None, None, Some(true)),
    );
}

#[test]
fn result_preserves_unicode_nul_and_newline_reason() {
    let result = BeforeToolCallResult {
        block: None,
        reason: Some("blocked\n理由🙂\0next".to_owned()),
        terminate: None,
    };

    assert_eq!(result.reason.as_deref(), Some("blocked\n理由🙂\0next"));
}

#[test]
fn result_preserves_pairwise_and_full_field_states() {
    let pairwise = [
        BeforeToolCallResult {
            block: Some(true),
            reason: Some("blocked".to_owned()),
            terminate: None,
        },
        BeforeToolCallResult {
            block: Some(true),
            reason: None,
            terminate: Some(true),
        },
        BeforeToolCallResult {
            block: None,
            reason: Some("hint".to_owned()),
            terminate: Some(false),
        },
    ];
    let full = BeforeToolCallResult {
        block: Some(true),
        reason: Some("blocked".to_owned()),
        terminate: Some(true),
    };

    assert_eq!(pairwise[0].reason.as_deref(), Some("blocked"));
    assert_eq!(pairwise[1].terminate, Some(true));
    assert_eq!(pairwise[2].reason.as_deref(), Some("hint"));
    assert_eq!(full.block, Some(true));
    assert_eq!(full.reason.as_deref(), Some("blocked"));
    assert_eq!(full.terminate, Some(true));
}

#[test]
fn result_fields_are_publicly_mutable() {
    let mut result = BeforeToolCallResult {
        block: None,
        reason: None,
        terminate: None,
    };

    result.block = Some(true);
    result.reason = Some("changed".to_owned());
    result.terminate = Some(false);

    assert_eq!(result.block, Some(true));
    assert_eq!(result.reason.as_deref(), Some("changed"));
    assert_eq!(result.terminate, Some(false));

    result.block = None;
    result.reason = None;
    result.terminate = None;

    assert!(result.block.is_none());
    assert!(result.reason.is_none());
    assert!(result.terminate.is_none());
}
