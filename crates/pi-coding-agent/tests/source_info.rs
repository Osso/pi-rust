use pi_coding_agent::core::source_info::{
    SourceInfo as ModuleSourceInfo, SourceOrigin, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info,
};
use pi_coding_agent::{SourceInfo, create_synthetic_source_info as create_from_root};
use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn source_info_exports_and_literals_are_exact() {
    assert_eq!(SourceScope::User.as_str(), "user");
    assert_eq!(SourceScope::Project.as_str(), "project");
    assert_eq!(SourceScope::Temporary.as_str(), "temporary");
    assert_eq!(SourceOrigin::Package.as_str(), "package");
    assert_eq!(SourceOrigin::TopLevel.as_str(), "top-level");

    let module_result = ModuleSourceInfo {
        path: "module".to_owned().into(),
        source: "source".to_owned().into(),
        scope: SourceScope::User,
        origin: SourceOrigin::Package,
        base_dir: None,
    };
    let root_result = SourceInfo {
        path: "root".to_owned().into(),
        source: "source".to_owned().into(),
        scope: SourceScope::Project,
        origin: SourceOrigin::TopLevel,
        base_dir: Some("/base".to_owned().into()),
    };

    assert_debug(&module_result);
    assert_debug(&root_result);
    assert_eq!(module_result.path, "module");
    assert_eq!(root_result.base_dir.as_deref(), Some("/base"));
}

#[test]
fn synthetic_source_info_applies_defaults_and_preserves_overrides() {
    let defaults = SyntheticSourceInfoOptions {
        source: "default-source".to_owned().into(),
        scope: None,
        origin: None,
        base_dir: None,
    };
    let explicit = SyntheticSourceInfoOptions {
        source: "代理🙂\0\n".to_owned().into(),
        scope: Some(SourceScope::User),
        origin: Some(SourceOrigin::Package),
        base_dir: Some("C:\\base\r\n".to_owned().into()),
    };

    let default_result = create_synthetic_source_info(&JsString::from(""), &defaults);
    let explicit_result = create_from_root(&JsString::from("旧\n"), &explicit);

    assert_eq!(default_result.scope, SourceScope::Temporary);
    assert_eq!(default_result.origin, SourceOrigin::TopLevel);
    assert!(default_result.base_dir.is_none());
    assert_eq!(explicit_result.path, "旧\n");
    assert_eq!(explicit_result.source, "代理🙂\0\n");
    assert_eq!(explicit_result.scope, SourceScope::User);
    assert_eq!(explicit_result.origin, SourceOrigin::Package);
    assert_eq!(explicit_result.base_dir.as_deref(), Some("C:\\base\r\n"));
}

#[test]
fn synthetic_source_info_returns_fresh_mutable_results_and_reuses_options() {
    let options = SyntheticSourceInfoOptions {
        source: "source".to_owned().into(),
        scope: Some(SourceScope::Project),
        origin: Some(SourceOrigin::TopLevel),
        base_dir: Some("/base".to_owned().into()),
    };

    let mut first = create_synthetic_source_info(&JsString::from("first"), &options);
    let second = create_synthetic_source_info(&JsString::from("second"), &options);

    first.path = "changed".to_owned().into();
    first.source = "changed-source".to_owned().into();
    first.scope = SourceScope::Temporary;
    first.origin = SourceOrigin::Package;
    first.base_dir = None;

    assert_eq!(second.path, "second");
    assert_eq!(second.source, "source");
    assert_eq!(second.scope, SourceScope::Project);
    assert_eq!(second.origin, SourceOrigin::TopLevel);
    assert_eq!(second.base_dir.as_deref(), Some("/base"));
    assert_eq!(options.source, "source");
    assert_eq!(options.scope, Some(SourceScope::Project));
    assert_eq!(options.origin, Some(SourceOrigin::TopLevel));
    assert_eq!(options.base_dir.as_deref(), Some("/base"));
}
