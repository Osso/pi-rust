use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_agent_core::harness::prompt_templates::{
    PromptTemplateDiagnostic as ModuleDiagnostic, PromptTemplateDiagnosticCode as ModuleCode,
    PromptTemplateDiagnosticType as ModuleType,
};
use pi_agent_core::harness::{PromptTemplateDiagnostic, PromptTemplateDiagnosticCode, PromptTemplateDiagnosticType};

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn diagnostic_codes_and_type_preserve_exact_literals() {
    let codes = [
        (ModuleCode::FileInfoFailed, "file_info_failed"),
        (ModuleCode::ListFailed, "list_failed"),
        (ModuleCode::ReadFailed, "read_failed"),
        (ModuleCode::ParseFailed, "parse_failed"),
    ];

    for (code, literal) in codes {
        assert_eq!(code.as_str(), literal);
        assert_eq!(code, code);
        let copied = code;
        assert_eq!(copied, code);
        assert_debug(&code);
    }

    let diagnostic_type = ModuleType::Warning;
    assert_eq!(diagnostic_type.as_str(), "warning");
    assert_eq!(diagnostic_type, ModuleType::Warning);
    let copied = diagnostic_type;
    assert_eq!(copied, diagnostic_type);
    assert_debug(&diagnostic_type);
}

#[test]
fn module_export_exposes_mutable_public_diagnostic_fields() {
    let mut diagnostic = ModuleDiagnostic {
        r#type: ModuleType::Warning,
        code: ModuleCode::FileInfoFailed,
        message: JsString::from("message"),
        path: JsString::from("/path"),
    };

    assert_eq!(diagnostic.r#type.as_str(), "warning");
    assert_eq!(diagnostic.code.as_str(), "file_info_failed");
    assert_eq!(diagnostic.message, "message");
    assert_eq!(diagnostic.path, "/path");

    diagnostic.r#type = ModuleType::Warning;
    diagnostic.code = ModuleCode::ParseFailed;
    diagnostic.message = JsString::from("updated message");
    diagnostic.path = JsString::from("/updated");

    assert_eq!(diagnostic.code, ModuleCode::ParseFailed);
    assert_eq!(diagnostic.message, "updated message");
    assert_eq!(diagnostic.path, "/updated");
    assert_debug(&diagnostic);
}

#[test]
fn root_export_exposes_mutable_public_diagnostic_fields() {
    let mut diagnostic = PromptTemplateDiagnostic {
        r#type: PromptTemplateDiagnosticType::Warning,
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: JsString::default(),
        path: JsString::default(),
    };

    diagnostic.code = PromptTemplateDiagnosticCode::ReadFailed;
    diagnostic.message = JsString::from("root message");
    diagnostic.path = JsString::from("root path");

    assert_eq!(diagnostic.r#type.as_str(), "warning");
    assert_eq!(diagnostic.code.as_str(), "read_failed");
    assert_eq!(diagnostic.message, "root message");
    assert_eq!(diagnostic.path, "root path");
    assert_debug(&diagnostic);
}

#[test]
fn diagnostic_preserves_empty_and_rust_string_boundaries() {
    let empty = ModuleDiagnostic {
        r#type: ModuleType::Warning,
        code: ModuleCode::FileInfoFailed,
        message: JsString::default(),
        path: JsString::default(),
    };
    assert!(empty.message.is_empty());
    assert!(empty.path.is_empty());

    let diagnostic = ModuleDiagnostic {
        r#type: ModuleType::Warning,
        code: ModuleCode::ParseFailed,
        message: JsString::from("ordinary 描述🙂e\u{301}\0\r\n<message>&\"'"),
        path: JsString::from("paths/路径/技能🙂e\u{301}\0\r\n<path>&\"'"),
    };

    assert_eq!(diagnostic.message, "ordinary 描述🙂e\u{301}\0\r\n<message>&\"'");
    assert_eq!(diagnostic.path, "paths/路径/技能🙂e\u{301}\0\r\n<path>&\"'");
}
