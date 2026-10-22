use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_agent_core::harness::messages::{BashExecutionMessage as ModuleMessage, BashExecutionRole as ModuleRole};
use pi_agent_core::harness::{BashExecutionMessage, BashExecutionRole};

const BASH_ROLE_TEXT: &str = ModuleRole::BashExecution.as_str();

fn assert_debug<T: Debug>(_: &T) {}

fn empty_message() -> ModuleMessage {
    ModuleMessage {
        role: ModuleRole::BashExecution,
        command: JsString::default(),
        output: JsString::default(),
        exit_code: None,
        cancelled: false,
        truncated: false,
        full_output_path: None,
        timestamp: 0.0,
        exclude_from_context: None,
    }
}

#[test]
fn role_has_exact_literal_and_closed_debug_copy_equality_contract() {
    assert_eq!(BASH_ROLE_TEXT, "bashExecution");

    let role = ModuleRole::BashExecution;
    let copied = role;
    assert_eq!(role, copied);
    assert_eq!(format!("{role:?}"), "BashExecution");
    assert_eq!(BashExecutionRole::BashExecution.as_str(), "bashExecution");
}

#[test]
fn module_and_root_exports_expose_the_same_public_struct_fields() {
    let mut module_message = empty_message();
    module_message.role = ModuleRole::BashExecution;
    module_message.command = JsString::from("echo module");
    module_message.output = JsString::from("module output");
    module_message.exit_code = Some(0.0);
    module_message.cancelled = true;
    module_message.truncated = true;
    module_message.full_output_path = Some(JsString::from("/tmp/module.log"));
    module_message.timestamp = 1.5;
    module_message.exclude_from_context = Some(false);

    let mut root_message = BashExecutionMessage {
        role: BashExecutionRole::BashExecution,
        command: JsString::default(),
        output: JsString::default(),
        exit_code: None,
        cancelled: false,
        truncated: false,
        full_output_path: None,
        timestamp: 0.0,
        exclude_from_context: None,
    };
    root_message.role = BashExecutionRole::BashExecution;
    root_message.command = module_message.command.clone();
    root_message.output = module_message.output.clone();
    root_message.exit_code = module_message.exit_code;
    root_message.cancelled = module_message.cancelled;
    root_message.truncated = module_message.truncated;
    root_message.full_output_path = module_message.full_output_path.clone();
    root_message.timestamp = module_message.timestamp;
    root_message.exclude_from_context = module_message.exclude_from_context;

    assert_eq!(root_message.role, BashExecutionRole::BashExecution);
    assert_eq!(root_message.command, "echo module");
    assert_eq!(root_message.output, "module output");
    assert_eq!(root_message.exit_code, Some(0.0));
    assert!(root_message.cancelled);
    assert!(root_message.truncated);
    assert_eq!(
        root_message.full_output_path.as_ref(),
        Some(&JsString::from("/tmp/module.log"))
    );
    assert_eq!(root_message.timestamp, 1.5);
    assert_eq!(root_message.exclude_from_context, Some(false));
    assert_debug(&module_message);
    assert_debug(&root_message);
}

#[test]
fn preserves_required_exit_code_and_optional_field_states() {
    let mut message = empty_message();
    assert_eq!(message.exit_code, None);
    assert_eq!(message.full_output_path, None);
    assert_eq!(message.exclude_from_context, None);

    message.exit_code = Some(17.0);
    message.full_output_path = Some(JsString::from("/tmp/full-output.log"));
    message.exclude_from_context = Some(false);
    assert_eq!(message.exit_code, Some(17.0));
    assert_eq!(
        message.full_output_path.as_ref(),
        Some(&JsString::from("/tmp/full-output.log"))
    );
    assert_eq!(message.exclude_from_context, Some(false));

    message.exclude_from_context = Some(true);
    assert_eq!(message.exclude_from_context, Some(true));
}

#[test]
fn preserves_empty_unicode_control_and_xml_sensitive_strings() {
    let mut message = empty_message();
    message.command = JsString::from("😀e\u{301}\0\r\n<&>\"'");
    message.output = JsString::from("");
    message.full_output_path = Some(JsString::from("路径/😀/e\u{301}\0\r\n<&>\"'"));

    assert_eq!(message.command, "😀e\u{301}\0\r\n<&>\"'");
    assert_eq!(message.output, "");
    assert_eq!(
        message.full_output_path.as_ref(),
        Some(&JsString::from("路径/😀/e\u{301}\0\r\n<&>\"'"))
    );
}

#[test]
fn preserves_binary64_bits_for_exit_code_and_timestamp() {
    let cases = [
        (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
        (0x8000_0000_0000_0000, 0x0000_0000_0000_0000),
        (0x7ff0_0000_0000_0000, 0xfff0_0000_0000_0000),
        (0xfff0_0000_0000_0000, 0x7ff0_0000_0000_0000),
        (0x0000_0000_0000_0001, 0x8000_0000_0000_0001),
        (0x8000_0000_0000_0001, 0x0000_0000_0000_0001),
        (0x7ff8_0000_0000_0001, 0x7ff8_0000_0000_0042),
        (0x7ff8_0000_0000_0042, 0x7ff8_0000_0000_0001),
    ];

    for (exit_bits, timestamp_bits) in cases {
        let message = ModuleMessage {
            exit_code: Some(f64::from_bits(exit_bits)),
            timestamp: f64::from_bits(timestamp_bits),
            ..empty_message()
        };

        assert_eq!(message.exit_code.expect("exit code is present").to_bits(), exit_bits);
        assert_eq!(message.timestamp.to_bits(), timestamp_bits);
    }
}
