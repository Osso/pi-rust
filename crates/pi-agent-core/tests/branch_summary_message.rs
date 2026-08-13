use std::fmt::Debug;

use pi_agent_core::harness::messages::{BranchSummaryMessage as ModuleMessage, BranchSummaryRole as ModuleRole};
use pi_agent_core::harness::{BranchSummaryMessage, BranchSummaryRole};

fn assert_debug<T: Debug>(_: &T) {}

fn empty_message() -> ModuleMessage {
    ModuleMessage {
        role: ModuleRole::BranchSummary,
        summary: String::new(),
        from_id: String::new(),
        timestamp: 0.0,
    }
}

#[test]
fn role_has_exact_literal_and_closed_debug_copy_equality_contract() {
    assert_eq!(ModuleRole::BranchSummary.as_str(), "branchSummary");

    let role = ModuleRole::BranchSummary;
    let copied = role;
    assert_eq!(role, copied);
    assert_eq!(format!("{role:?}"), "BranchSummary");
    assert_eq!(BranchSummaryRole::BranchSummary.as_str(), "branchSummary");
}

#[test]
fn module_and_root_exports_expose_mutable_public_fields() {
    let mut module_message = empty_message();
    module_message.role = ModuleRole::BranchSummary;
    module_message.summary = "module summary".to_owned();
    module_message.from_id = "module-id".to_owned();
    module_message.timestamp = 1.5;

    let mut root_message = BranchSummaryMessage {
        role: BranchSummaryRole::BranchSummary,
        summary: String::new(),
        from_id: String::new(),
        timestamp: 0.0,
    };
    root_message.role = BranchSummaryRole::BranchSummary;
    root_message.summary = module_message.summary.clone();
    root_message.from_id = module_message.from_id.clone();
    root_message.timestamp = module_message.timestamp;

    assert_eq!(module_message.role, ModuleRole::BranchSummary);
    assert_eq!(module_message.summary, "module summary");
    assert_eq!(module_message.from_id, "module-id");
    assert_eq!(module_message.timestamp, 1.5);
    assert_eq!(root_message.role, BranchSummaryRole::BranchSummary);
    assert_eq!(root_message.summary, "module summary");
    assert_eq!(root_message.from_id, "module-id");
    assert_eq!(root_message.timestamp, 1.5);
    assert_debug(&module_message);
    assert_debug(&root_message);
}

#[test]
fn preserves_empty_unicode_control_and_xml_sensitive_strings() {
    let mut message = empty_message();
    message.summary = "😀e\u{301}\0\r\n<&>\"'".to_owned();
    message.from_id = "路径/😀/e\u{301}\0\r\n<&>\"'".to_owned();

    assert_eq!(message.summary, "😀e\u{301}\0\r\n<&>\"'");
    assert_eq!(message.from_id, "路径/😀/e\u{301}\0\r\n<&>\"'");

    message.summary.clear();
    message.from_id.clear();
    assert!(message.summary.is_empty());
    assert!(message.from_id.is_empty());
}

#[test]
fn preserves_timestamp_binary64_bits() {
    let timestamp_bits = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x7fefffff_ffff_ffff,
        0x7ff8_0000_0000_0001,
        0x7ff8_0000_0000_0042,
        0xfff8_0000_0000_0001,
        0xfff8_0000_0000_0042,
    ];

    for bits in timestamp_bits {
        let message = ModuleMessage {
            timestamp: f64::from_bits(bits),
            ..empty_message()
        };
        assert_eq!(message.timestamp.to_bits(), bits);
    }
}
