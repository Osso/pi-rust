//! Behavioral tests for `modes/interactive/components/config-selector.ts`.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_coding_agent::core::package_manager::{
    MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions, ConfigWriteScope, ScopedResolvedPaths,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::Component;
use pi_tui::utils::strip_terminal_sequences;
use serde_json::{Value, json};
use tempfile::TempDir;

fn extension_paths(package_root: &str, source: &str, scope: SourceScope, names: &[&str]) -> ResolvedPaths {
    ResolvedPaths {
        extensions: names
            .iter()
            .map(|name| ResolvedResource {
                path: JsString::from(format!("{package_root}/extensions/{name}")),
                enabled: true,
                metadata: PathMetadata {
                    source: JsString::from(source.to_string()),
                    scope,
                    origin: MetadataOrigin::Package,
                    base_dir: Some(JsString::from(package_root.to_string())),
                },
            })
            .collect(),
        ..Default::default()
    }
}

fn settings_with_global_packages() -> Settings {
    let mut settings = Settings::new();
    settings.insert(JsString::from("packages"), JsValue::from(json!(["npm:pi-tools"])));
    settings
}

fn project_packages(manager: &SettingsManager) -> JsValue {
    manager
        .get_project_settings()
        .get("packages")
        .cloned()
        .unwrap_or(JsValue::from(Value::Null))
}

fn top_level_paths(package_root: &str, names: &[&str], agent_dir: &str) -> ResolvedPaths {
    ResolvedPaths {
        extensions: names
            .iter()
            .map(|name| ResolvedResource {
                path: JsString::from(format!("{package_root}/extensions/{name}")),
                enabled: true,
                metadata: PathMetadata {
                    source: JsString::from("auto"),
                    scope: SourceScope::User,
                    origin: MetadataOrigin::TopLevel,
                    base_dir: Some(JsString::from(agent_dir.to_string())),
                },
            })
            .collect(),
        ..Default::default()
    }
}

fn install_theme() {
    let colors: Vec<(JsString, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                ))),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme);
}

fn render_lines(selector: &mut ConfigSelectorComponent) -> Vec<JsString> {
    install_theme();
    selector
        .render(100)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect()
}

fn selector(
    temp: &TempDir,
    settings_manager: Rc<RefCell<SettingsManager>>,
    resolved: ResolvedPaths,
) -> ConfigSelectorComponent {
    selector_with_scope(temp, settings_manager, resolved, ConfigWriteScope::Project, true)
}

fn selector_with_scope(
    temp: &TempDir,
    settings_manager: Rc<RefCell<SettingsManager>>,
    resolved: ResolvedPaths,
    write_scope: ConfigWriteScope,
    project_mode_available: bool,
) -> ConfigSelectorComponent {
    let project_dir = temp.path().join("project");
    let agent_dir = temp.path().join("agent");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    ConfigSelectorComponent::new(ConfigSelectorOptions {
        resolved_paths: ScopedResolvedPaths {
            global: resolved.clone(),
            project: resolved,
        },
        settings_manager,
        cwd: JsString::from(project_dir.to_string_lossy().into_owned()),
        agent_dir: JsString::from(agent_dir.to_string_lossy().into_owned()),
        on_close: Rc::new(|| {}),
        on_exit: Rc::new(|| {}),
        request_render: Rc::new(|| {}),
        terminal_height: Some(24),
        write_scope,
        project_mode_available,
    })
}

#[test]
fn cycles_project_package_overrides_in_config_local_mode() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        settings_with_global_packages(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let package_root = temp.path().join("pkg");
    let resolved = extension_paths(
        &package_root.to_string_lossy(),
        "npm:pi-tools",
        SourceScope::User,
        &["bar.ts"],
    );
    let mut selector = selector(&temp, manager.clone(), resolved);

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        project_packages(&manager.borrow()),
        pi_js_compat::js_json!([{ "source": "npm:pi-tools", "autoload": false, "extensions": ["-extensions/bar.ts"] }])
    );

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        project_packages(&manager.borrow()),
        pi_js_compat::js_json!([{ "source": "npm:pi-tools", "autoload": false, "extensions": ["+extensions/bar.ts"] }])
    );

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(project_packages(&manager.borrow()), pi_js_compat::js_json!([]));
}

#[test]
fn tab_switches_write_scope_and_updates_header() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        settings_with_global_packages(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let package_root = temp.path().join("pkg");
    let resolved = extension_paths(
        &package_root.to_string_lossy(),
        "npm:pi-tools",
        SourceScope::User,
        &["bar.ts"],
    );
    let mut selector = selector_with_scope(&temp, manager.clone(), resolved, ConfigWriteScope::Global, true);

    assert!(
        render_lines(&mut selector)
            .iter()
            .any(|line| line.contains("Global Resources"))
    );

    selector.handle_input(&JsString::from("\t"));
    let lines = render_lines(&mut selector);
    assert!(lines.iter().any(|line| line.contains("Project Local Resources")));

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        project_packages(&manager.borrow()),
        pi_js_compat::js_json!([{ "source": "npm:pi-tools", "autoload": false, "extensions": ["-extensions/bar.ts"] }])
    );
    assert_eq!(
        manager.borrow().get_global_settings().get("packages"),
        Some(&json!(["npm:pi-tools"])).map(JsValue::from).as_ref()
    );
}

#[test]
fn tab_is_ignored_when_project_mode_is_unavailable() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        settings_with_global_packages(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let package_root = temp.path().join("pkg");
    let resolved = extension_paths(
        &package_root.to_string_lossy(),
        "npm:pi-tools",
        SourceScope::User,
        &["bar.ts"],
    );
    let mut selector = selector_with_scope(&temp, manager.clone(), resolved, ConfigWriteScope::Global, false);

    selector.handle_input(&JsString::from("\t"));
    let lines = render_lines(&mut selector);
    assert!(lines.iter().any(|line| line.contains("Global Resources")));
    assert!(!lines.iter().any(|line| line.contains("Project Local Resources")));
}

#[test]
fn toggles_global_top_level_extension_paths() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        Settings::new(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let agent_dir = temp.path().join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let resolved = top_level_paths(&agent_dir.to_string_lossy(), &["foo.ts"], &agent_dir.to_string_lossy());
    let mut selector = selector_with_scope(&temp, manager.clone(), resolved, ConfigWriteScope::Global, true);

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        manager.borrow().get_global_settings().get("extensions"),
        Some(&json!(["-extensions/foo.ts"])).map(JsValue::from).as_ref()
    );

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        manager.borrow().get_global_settings().get("extensions"),
        Some(&json!(["+extensions/foo.ts"])).map(JsValue::from).as_ref()
    );
}

#[test]
fn cycles_project_overrides_for_inherited_global_top_level_resources() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        Settings::new(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let agent_dir = temp.path().join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let resolved = top_level_paths(&agent_dir.to_string_lossy(), &["foo.ts"], &agent_dir.to_string_lossy());
    let path = format!("{}/extensions/foo.ts", agent_dir.to_string_lossy());
    let mut selector = selector(&temp, manager.clone(), resolved);

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        manager.borrow().get_project_settings().get("extensions"),
        Some(&json!([path, format!("-{path}")])).map(JsValue::from).as_ref()
    );

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        manager.borrow().get_project_settings().get("extensions"),
        Some(&json!([path, format!("+{path}")])).map(JsValue::from).as_ref()
    );

    selector.get_resource_list_mut().handle_input(&JsString::from(" "));
    assert_eq!(
        manager.borrow().get_project_settings().get("extensions"),
        Some(&json!([])).map(JsValue::from).as_ref()
    );
}

#[test]
fn renders_inherited_global_items_as_dimmed_in_project_mode() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        Settings::new(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let package_root = temp.path().join("pkg");
    let resolved = extension_paths(
        &package_root.to_string_lossy(),
        "npm:pi-tools",
        SourceScope::User,
        &["bar.ts"],
    );
    let mut selector = selector(&temp, manager, resolved);

    let lines = render_lines(&mut selector);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("npm:pi-tools (user) · inherited global")),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("    [x] bar.ts  inherited global")),
        "{lines:#?}"
    );
}

#[test]
fn search_filters_resources_and_reports_no_matches() {
    let temp = TempDir::new().unwrap();
    let manager = Rc::new(RefCell::new(SettingsManager::in_memory(
        Settings::new(),
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));
    let package_root = temp.path().join("pkg");
    let resolved = extension_paths(
        &package_root.to_string_lossy(),
        "npm:pi-tools",
        SourceScope::User,
        &["bar.ts", "baz.ts"],
    );
    let mut selector = selector_with_scope(&temp, manager, resolved, ConfigWriteScope::Global, true);

    for character in "baz".chars() {
        selector
            .get_resource_list_mut()
            .handle_input(&JsString::from(&character.to_string()));
    }
    let lines = render_lines(&mut selector);
    assert!(lines.iter().any(|line| line.contains("baz.ts")), "{lines:#?}");
    assert!(!lines.iter().any(|line| line.contains("bar.ts")), "{lines:#?}");

    for character in "zzz".chars() {
        selector
            .get_resource_list_mut()
            .handle_input(&JsString::from(&character.to_string()));
    }
    let lines = render_lines(&mut selector);
    assert!(
        lines.iter().any(|line| line.contains("No resources found")),
        "{lines:#?}"
    );
}
