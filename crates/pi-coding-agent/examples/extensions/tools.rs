//! Opt-in interactive tool selection, persisted in branch-local tools-config entries.
use indexmap::IndexSet;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor, get_settings_list_theme};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{
    components::settings_list::{SettingItem, SettingsList, SettingsListOptions},
    tui::Component,
};
use serde_json::json;
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

pub fn tools_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let enabled = Arc::new(Mutex::new(IndexSet::<String>::new()));
    let api = pi.clone();
    let start = enabled.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let enabled = start.clone();
            Box::pin(async move {
                restore(&api, &enabled, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    let tree = enabled.clone();
    pi.on_session_tree(
        move |_: SessionTreeEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let enabled = tree.clone();
            Box::pin(async move {
                restore(&api, &enabled, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    pi.register_command(
        "tools",
        Some("Enable/disable tools".into()),
        None,
        Arc::new(move |_, ctx| {
            let api = api.clone();
            let enabled = enabled.clone();
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?
                        .notify(&JsString::from("/tools requires TUI mode"), Some(NotifyType::Error))?;
                    return Ok(());
                }
                let names = api.get_all_tools()?.into_iter().map(|t| t.name).collect::<Vec<_>>();
                let error = Arc::new(Mutex::new(None));
                let failure = error.clone();
                ctx.ui()?
                    .custom(
                        Arc::new(move |context, done| {
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(ToolSelector {
                                names: names.clone(),
                                enabled: enabled.clone(),
                                api: api.clone(),
                                cursor: Mutex::default(),
                                theme: context.theme,
                                tui: context.tui,
                                done,
                                error: failure.clone(),
                            }))
                        }),
                        None,
                    )
                    .await?;
                if let Some(error) = error.lock().unwrap().take() {
                    return Err(error);
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
fn restore(pi: &ExtensionApi, enabled: &Mutex<IndexSet<String>>, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let all = pi.get_all_tools()?;
    let branch = ctx.session_manager()?.get_branch(None);
    let mut saved = None;
    for entry in branch {
        let entry = serde_json::to_value(entry).map_err(|e| HandlerError::new(e.to_string()))?;
        if entry["type"] == "custom"
            && entry["customType"] == "tools-config"
            && entry["data"]["enabledTools"].is_array()
        {
            saved = Some(entry["data"]["enabledTools"].clone());
        }
    }
    let values = match saved {
        Some(saved) => {
            let values: Vec<String> = serde_json::from_value(saved).map_err(|e| HandlerError::new(e.to_string()))?;
            let values: IndexSet<_> = values
                .into_iter()
                .filter(|name| all.iter().any(|tool| &tool.name == name))
                .collect();
            pi.set_active_tools(values.iter().cloned().collect())?;
            values
        }
        None => pi.get_active_tools()?.into_iter().collect(),
    };
    *enabled.lock().unwrap() = values;
    Ok(())
}
struct ToolSelector {
    names: Vec<String>,
    enabled: Arc<Mutex<IndexSet<String>>>,
    api: ExtensionApi,
    cursor: Mutex<Cursor>,
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    error: Arc<Mutex<Option<HandlerError>>>,
}
impl ToolSelector {
    // Native TUI components use Rc; keep only the selection snapshot across the
    // Send+Sync extension boundary and reconstruct the non-search SettingsList
    // locally. Native rendering/keybindings remain the source of truth.
    fn replay_selection(&self, list: &mut SettingsList) {
        let cursor = self.cursor.lock().unwrap();
        if cursor.index == 0 {
            return;
        }
        let (key, down) = cursor
            .navigation
            .as_ref()
            .expect("selected cursor has navigation input");
        let steps = if *down {
            cursor.index
        } else {
            self.names.len() - cursor.index
        };
        for _ in 0..steps {
            list.handle_input(&JsString::from(key));
        }
    }
    fn list(&self) -> SettingsList {
        let enabled = self.enabled.lock().unwrap();
        let items = self
            .names
            .iter()
            .map(|name| SettingItem {
                id: JsString::from(name.clone()),
                label: JsString::from(name.clone()),
                description: None,
                current_value: if enabled.contains(name) { "enabled" } else { "disabled" }.into(),
                values: Some(vec!["enabled".into(), "disabled".into()]),
                submenu: None,
            })
            .collect();
        drop(enabled);
        let api = self.api.clone();
        let enabled = self.enabled.clone();
        let done = self.done.clone();
        let close = done.clone();
        let error = self.error.clone();
        let mut list = SettingsList::new(
            items,
            (self.names.len() + 2).min(15),
            get_settings_list_theme(),
            Rc::new(move |id, value| {
                let names = {
                    let mut enabled = enabled.lock().unwrap();
                    if value == "enabled" {
                        enabled.insert(id.to_utf8().expect("selected tool ID must be a Unicode scalar string"));
                    } else {
                        enabled.shift_remove(&id.to_utf8().expect("selected tool ID must be a Unicode scalar string"));
                    }
                    enabled.iter().cloned().collect::<Vec<_>>()
                };
                let result = api.set_active_tools(names.clone()).and_then(|()| {
                    api.append_entry(
                        &JsString::from("tools-config"),
                        Some(JsValue::from(json!({"enabledTools":names}))),
                    )
                });
                if let Err(failure) = result {
                    *error.lock().unwrap() = Some(failure);
                    done.call(None);
                }
            }),
            Rc::new(move || close.call(None)),
            SettingsListOptions::default(),
        );
        self.replay_selection(&mut list);
        list
    }
}
impl ExtensionUiComponent for ToolSelector {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut lines = vec![
            self.theme.fg(ThemeColor::Accent, self.theme.bold("Tool Configuration")),
            JsString::default(),
        ];
        lines.extend(self.list().render(width));
        lines
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let mut list = self.list();
        list.handle_input(data);
        let keys = pi_tui::keybindings::get_keybindings();
        let down = keys.matches(data, "tui.select.down");
        let up = keys.matches(data, "tui.select.up");
        let mut cursor = self.cursor.lock().unwrap();
        cursor.index = list.selected_index();
        if down || up {
            cursor.navigation = Some((data.into(), down));
        }
        drop(cursor);
        if self.error.lock().unwrap().is_none() {
            self.tui.request_render();
        }
    }
}

#[derive(Default)]
struct Cursor {
    index: usize,
    navigation: Option<(JsString, bool)>,
}
