//! Native resource toggles and UTF-16 behavior; exact frames also run against frozen TS.
use std::cell::RefCell;
use std::rc::Rc;

use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions,
};
use pi_coding_agent::modes::interactive::theme::init_theme;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::{js_format, js_json};
use pi_tui::utils::strip_terminal_sequences;
use pi_tui::{Component, Focusable};

fn selector(resolved: ResolvedPaths, manager: Rc<RefCell<SettingsManager>>) -> ConfigSelectorComponent {
    init_theme(None, false).unwrap();
    ConfigSelectorComponent::new(ConfigSelectorOptions {
        resolved_paths: resolved,
        settings_manager: manager,
        cwd: "/tmp/pi-config-native/project".into(),
        agent_dir: "/tmp/pi-config-native/agent".into(),
        on_close: Rc::new(|| {}),
        on_exit: Rc::new(|| {}),
        request_render: Rc::new(|| {}),
        terminal_height: Some(24),
    })
}

fn manager(settings: Settings) -> Rc<RefCell<SettingsManager>> {
    Rc::new(RefCell::new(SettingsManager::in_memory(
        settings,
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )))
}

fn resource(
    path: JsString,
    scope: SourceScope,
    origin: MetadataOrigin,
    source: &str,
    base: Option<JsString>,
) -> ResolvedPaths {
    ResolvedPaths {
        extensions: vec![ResolvedResource {
            path,
            enabled: true,
            metadata: PathMetadata {
                source: source.into(),
                scope,
                origin,
                base_dir: base,
            },
        }],
        ..Default::default()
    }
}

#[test]
fn package_toggles_write_only_the_resource_scope() {
    let settings: Settings = js_json::from_value(&js_json!({"packages":["npm:tools"]})).unwrap();
    let manager = manager(settings);
    let resolved = resource(
        "/tmp/pi-config-native/pkg/extensions/bar.ts".into(),
        SourceScope::User,
        MetadataOrigin::Package,
        "npm:tools",
        Some("/tmp/pi-config-native/pkg".into()),
    );
    let mut selector = selector(resolved, Rc::clone(&manager));
    for pattern in ["-extensions/bar.ts", "+extensions/bar.ts", "-extensions/bar.ts"] {
        selector.handle_input(&" ".into());
        assert_eq!(
            manager.borrow().get_global_settings().get("packages"),
            Some(&js_json!([{"source":"npm:tools","extensions":[pattern]}]))
        );
        assert_eq!(manager.borrow().get_project_settings().get("packages"), None);
    }
}

#[test]
fn project_top_level_toggle_replaces_only_its_exact_patterns() {
    let manager = manager(Settings::new());
    manager
        .borrow_mut()
        .set_project_extension_paths(&[
            "!extensions/local.ts".into(),
            "+extensions/local.ts".into(),
            "keep.ts".into(),
        ])
        .unwrap();
    let resolved = resource(
        "/tmp/pi-config-native/project/.pi/extensions/local.ts".into(),
        SourceScope::Project,
        MetadataOrigin::TopLevel,
        "auto",
        None,
    );
    let mut selector = selector(resolved, Rc::clone(&manager));
    for pattern in ["-extensions/local.ts", "+extensions/local.ts"] {
        selector.handle_input(&"\r".into());
        assert_eq!(
            manager.borrow().get_project_settings().get("extensions"),
            Some(&js_json!(["keep.ts", pattern]))
        );
        assert_eq!(manager.borrow().get_global_settings().get("extensions"), None);
    }
}

#[test]
fn raw_resource_name_query_and_persisted_pattern_keep_exact_units() {
    let raw = JsString::from_utf16(vec![b'x' as u16, 0xd800, b'y' as u16]);
    let base = JsString::from("/tmp/pi-config-native/agent");
    let path = js_format!("{}/extensions/{}.ts", base, raw);
    let resolved = resource(path, SourceScope::User, MetadataOrigin::TopLevel, "auto", Some(base));
    let manager = manager(Settings::new());
    let mut selector = selector(resolved, Rc::clone(&manager));
    selector.set_focused(true);
    selector.handle_input(&raw);
    let lines = selector.render(100);
    assert!(lines.iter().any(|line| strip_terminal_sequences(line).contains(&raw)));
    selector.handle_input(&" ".into());
    assert_eq!(
        manager.borrow().get_global_settings().get("extensions"),
        Some(&JsValue::Array(vec![JsValue::String(js_format!(
            "-extensions/{}.ts",
            raw
        ))]))
    );
    selector.invalidate();
    assert!(
        selector
            .render(100)
            .iter()
            .any(|line| strip_terminal_sequences(line).contains(&raw))
    );
}

#[test]
fn tab_does_not_change_resource_scope_or_header() {
    let manager = manager(Settings::new());
    let resolved = resource(
        "/tmp/pi-config-native/agent/extensions/a.ts".into(),
        SourceScope::User,
        MetadataOrigin::TopLevel,
        "auto",
        None,
    );
    let mut selector = selector(resolved, Rc::clone(&manager));
    selector.handle_input(&"\t".into());
    let lines = selector.render(100);
    assert_eq!(
        strip_terminal_sequences(&lines[3]),
        JsString::from(
            "Resource Configuration                                                      space toggle · esc close"
        )
    );
    selector.handle_input(&" ".into());
    assert_eq!(
        manager.borrow().get_global_settings().get("extensions"),
        Some(&js_json!(["-extensions/a.ts"]))
    );
    assert_eq!(manager.borrow().get_project_settings().get("extensions"), None);
}
