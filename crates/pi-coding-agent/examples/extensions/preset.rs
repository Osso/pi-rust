//! Opt-in native preset example. No default loading; config is read on session_start.
use indexmap::IndexMap;
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::Model;
use pi_coding_agent::{
    config::{config_dir_name, get_agent_dir},
    core::extensions::*,
    modes::interactive::theme::{Theme, ThemeColor},
};
use pi_js_compat::js_format;
use pi_js_compat::js_json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use pi_tui::{
    Component, Text,
    components::{
        dynamic_border::DynamicBorder,
        select_list::{SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme},
    },
};
use serde::Deserialize;
use serde_json::json;
use std::{
    path::Path,
    rc::Rc,
    sync::{Arc, Mutex},
};
type Logger = Arc<dyn Fn(JsString) + Send + Sync>;
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preset {
    provider: Option<String>,
    model: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    tools: Option<Vec<String>>,
    instructions: Option<JsString>,
}
#[derive(Clone)]
struct Original {
    model: Option<Model>,
    thinking: ThinkingLevel,
    tools: Vec<String>,
}
#[derive(Default)]
struct State {
    presets: IndexMap<JsString, Preset>,
    active: Option<JsString>,
    preset: Option<Preset>,
    original: Option<Original>,
}
type Shared = Arc<Mutex<State>>;
pub fn preset_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    register_with_logger(pi, Arc::new(|message| eprintln!("{}", encode_node_utf8(&message))))
}
pub(crate) fn register_with_logger(pi: &ExtensionApi, logger: Logger) -> Result<(), HandlerError> {
    let state: Shared = Arc::default();
    pi.register_flag(
        "preset",
        Some("Preset configuration to use".into()),
        FlagType::String,
        None,
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.register_shortcut(
        "ctrl+shift+u".into(),
        Some("Cycle presets".into()),
        Arc::new(move |ctx| {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move { cycle(&api, &shared, &ctx).await })
        }),
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.register_command(
        "preset",
        Some("Switch preset configuration".into()),
        None,
        Arc::new(move |args, ctx| {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move {
                let name = args.trim();
                if name.is_empty() {
                    return select(&api, &shared, &ctx).await;
                }
                let preset = shared.lock().unwrap().presets.get(&name).cloned();
                if let Some(preset) = preset {
                    apply(&api, &shared, &name, preset, &ctx).await?;
                    ctx.ui()?
                        .notify(&js_format!("Preset \"{}\" activated", name), Some(NotifyType::Info))?;
                    status(&shared, &ctx)?;
                } else {
                    let available = available(&shared);
                    ctx.ui()?.notify(
                        &js_format!("Unknown preset \"{}\". Available: {}", name, available),
                        Some(NotifyType::Error),
                    )?;
                }
                Ok(())
            })
        }),
    )?;
    let shared = state.clone();
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              _: ExtensionContext|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let instructions = shared
                .lock()
                .unwrap()
                .preset
                .as_ref()
                .and_then(|p| p.instructions.clone())
                .filter(|s| !s.is_empty());
            Box::pin(async move {
                Ok(instructions.map(|instructions| BeforeAgentStartEventResult {
                    message: None,
                    system_prompt: Some(js_format!("{}\n\n{}", event.system_prompt, instructions)),
                }))
            })
        },
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let shared = shared.clone();
            let logger = logger.clone();
            Box::pin(async move {
                let presets = load_presets(&ctx.cwd()?, &logger)?;
                shared.lock().unwrap().presets = presets;
                let flag = api.get_flag("preset")?;
                let name = flag.as_ref().and_then(FlagValue::as_str).filter(|s| !s.is_empty());
                if let Some(name) = name {
                    let preset = shared.lock().unwrap().presets.get(name).cloned();
                    if let Some(preset) = preset {
                        apply(&api, &shared, &JsString::from(name), preset, &ctx).await?;
                        ctx.ui()?.notify(
                            &JsString::from(format!("Preset \"{name}\" activated")),
                            Some(NotifyType::Info),
                        )?;
                    } else {
                        ctx.ui()?.notify(
                            &js_format!("Unknown preset \"{}\". Available: {}", name, available(&shared)),
                            Some(NotifyType::Warning),
                        )?;
                    }
                }
                let entries = ctx.session_manager()?.get_entries();
                let saved = entries
                    .iter()
                    .rev()
                    .filter_map(|entry| {
                        let value = serde_json::to_value(entry).expect("session entry");
                        if value["type"] == "custom" && value["customType"] == "preset-state" {
                            Some(value)
                        } else {
                            None
                        }
                    })
                    .next();
                if !flag.as_ref().is_some_and(|f| match f {
                    FlagValue::String(s) => !s.is_empty(),
                    FlagValue::Boolean(b) => *b,
                }) && let Some(name) = saved
                    .as_ref()
                    .and_then(|s| s["data"]["name"].as_str())
                    .filter(|s| !s.is_empty())
                {
                    let mut state = shared.lock().unwrap();
                    if let Some(preset) = state.presets.get(name).cloned() {
                        state.active = Some(name.into());
                        state.preset = Some(preset);
                    }
                }
                status(&shared, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    let shared = state;
    pi.on_turn_start(
        move |_: TurnStartEvent, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let result = (|| {
                if let Some(name) = &shared.lock().unwrap().active {
                    api.append_entry(
                        &JsString::from("preset-state"),
                        Some(JsValue::from(json!({"name":name}))),
                    )?;
                }
                Ok(None)
            })();
            Box::pin(async move { result })
        },
    )?;
    Ok(())
}
fn load_file(path: &JsString, label: &str, logger: &Logger) -> Result<IndexMap<JsString, Preset>, HandlerError> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(IndexMap::new());
    }
    let content = match std::fs::read(encode_node_utf8(path)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => {
            logger(js_format!(
                "Failed to load {} presets from {}: Error: {}",
                label,
                path,
                pi_js_compat::node_fs::node_read_error(&error, path)
            ));
            return Ok(IndexMap::new());
        }
    };
    let data = match js_json::parse_lossless(&content) {
        Ok(data) => data,
        Err(error) => {
            logger(js_format!(
                "Failed to load {} presets from {}: SyntaxError: {}",
                label,
                path,
                error.message()
            ));
            return Ok(IndexMap::new());
        }
    };
    let object = data
        .as_object()
        .ok_or_else(|| HandlerError::new("presets must be an object"))?;
    js_json::object_entries_lossless(object)
        .into_iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                js_json::from_value(value).map_err(|e| HandlerError::new(e.to_string()))?,
            ))
        })
        .collect()
}
fn load_presets(cwd: &JsString, logger: &Logger) -> Result<IndexMap<JsString, Preset>, HandlerError> {
    let mut presets = load_file(
        &pi_coding_agent::utils::paths::node_join(get_agent_dir()?, "presets.json"),
        "global",
        logger,
    )?;
    presets.extend(load_file(
        &pi_coding_agent::utils::paths::node_join(
            pi_coding_agent::utils::paths::node_join(cwd, config_dir_name()),
            "presets.json",
        ),
        "project",
        logger,
    )?);
    // Object spread enumerates integer-index keys before insertion-ordered names.
    let mut entries: Vec<_> = presets.into_iter().collect();
    entries.sort_by_key(|(key, _)| index_key(key).map_or((1, 0), |index| (0, index)));
    Ok(entries.into_iter().collect())
}
fn index_key(key: &JsString) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.units()[0] == u16::from(b'0')) {
        return None;
    }
    let number = key.units().iter().try_fold(0u32, |number, unit| {
        if !(u16::from(b'0')..=u16::from(b'9')).contains(unit) {
            return None;
        }
        number.checked_mul(10)?.checked_add(u32::from(*unit - u16::from(b'0')))
    })?;
    (number != u32::MAX).then_some(number)
}

fn available(shared: &Shared) -> JsString {
    let names = JsString::join(shared.lock().unwrap().presets.keys().cloned().collect::<Vec<_>>(), ", ");
    if names.is_empty() {
        "(none defined)".into()
    } else {
        names
    }
}
fn status(shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let name = shared.lock().unwrap().active.clone();
    let text = name
        .filter(|s| !s.is_empty())
        .map(|name| {
            ui.theme()
                .map(|theme| theme.fg(ThemeColor::Accent, &js_format!("preset:{}", name)))
        })
        .transpose()?;
    ui.set_status(&JsString::from("preset"), text.map(ExtensionStatus::Text))?;
    Ok(())
}
async fn apply(
    pi: &ExtensionApi,
    shared: &Shared,
    name: &JsString,
    preset: Preset,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    if shared.lock().unwrap().active.is_none() {
        let original = Original {
            model: ctx.model()?,
            thinking: pi.get_thinking_level()?,
            tools: pi.get_active_tools()?,
        };
        shared.lock().unwrap().original = Some(original);
    }
    if let (Some(provider), Some(id)) = (&preset.provider, &preset.model)
        && !provider.is_empty()
        && !id.is_empty()
    {
        if let Some(model) = ctx.model_registry()?.find(provider, id) {
            if !pi.set_model(model).await? {
                ctx.ui()?.notify(
                    &js_format!("Preset \"{}\": No API key for {}/{}", name, provider, id),
                    Some(NotifyType::Warning),
                )?;
            }
        } else {
            ctx.ui()?.notify(
                &js_format!("Preset \"{}\": Model {}/{} not found", name, provider, id),
                Some(NotifyType::Warning),
            )?;
        }
    }
    if let Some(thinking) = preset.thinking_level {
        pi.set_thinking_level(thinking)?;
    }
    if let Some(tools) = &preset.tools
        && !tools.is_empty()
    {
        let all = pi.get_all_tools()?.into_iter().map(|t| t.name).collect::<Vec<_>>();
        let (valid, invalid): (Vec<_>, Vec<_>) = tools.iter().cloned().partition(|name| all.contains(name));
        if !invalid.is_empty() {
            ctx.ui()?.notify(
                &js_format!("Preset \"{}\": Unknown tools: {}", name, invalid.join(", ")),
                Some(NotifyType::Warning),
            )?;
        }
        if !valid.is_empty() {
            pi.set_active_tools(valid)?;
        }
    }
    let mut state = shared.lock().unwrap();
    state.active = Some(name.into());
    state.preset = Some(preset);
    Ok(())
}
async fn clear(pi: &ExtensionApi, shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let original = {
        let mut state = shared.lock().unwrap();
        state.active = None;
        state.preset = None;
        state.original.clone()
    };
    if let Some(original) = original {
        if let Some(model) = original.model {
            pi.set_model(model).await?;
        }
        pi.set_thinking_level(original.thinking)?;
        pi.set_active_tools(original.tools)?;
    } else {
        pi.set_active_tools(["read", "bash", "edit", "write"].map(str::to_owned).into())?;
    }
    ctx.ui()?.notify(
        &JsString::from("Preset cleared, defaults restored"),
        Some(NotifyType::Info),
    )?;
    status(shared, ctx)
}
fn no_presets(ctx: &ExtensionContext) -> Result<(), HandlerError> {
    ctx.ui()?.notify(
        &js_format!(
            "No presets defined. Add presets to {} or {}",
            pi_coding_agent::utils::paths::node_join(get_agent_dir()?, "presets.json"),
            pi_coding_agent::utils::paths::node_join(
                pi_coding_agent::utils::paths::node_join(ctx.cwd()?, config_dir_name()),
                "presets.json"
            )
        ),
        Some(NotifyType::Warning),
    )?;
    Ok(())
}
async fn cycle(pi: &ExtensionApi, shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let (mut names, current) = {
        let state = shared.lock().unwrap();
        (state.presets.keys().cloned().collect::<Vec<_>>(), state.active.clone())
    };
    if names.is_empty() {
        return no_presets(ctx);
    }
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names.insert(0, "(none)".into());
    let current = current.unwrap_or_else(|| JsString::from("(none)"));
    let next = names
        .iter()
        .position(|name| name == &current)
        .map_or(0, |i| (i + 1) % names.len());
    let name = &names[next];
    if name == "(none)" {
        return clear(pi, shared, ctx).await;
    }
    let preset = shared.lock().unwrap().presets.get(name).cloned();
    if let Some(preset) = preset {
        apply(pi, shared, &JsString::from(name), preset, ctx).await?;
        ctx.ui()?
            .notify(&js_format!("Preset \"{}\" activated", name), Some(NotifyType::Info))?;
        status(shared, ctx)?;
    }
    Ok(())
}
fn description(preset: &Preset) -> JsString {
    let mut parts = Vec::new();
    if let (Some(provider), Some(model)) = (&preset.provider, &preset.model)
        && !provider.is_empty()
        && !model.is_empty()
    {
        parts.push(js_format!("{}/{}", provider, model));
    }
    if let Some(thinking) = preset.thinking_level {
        parts.push(js_format!(
            "thinking:{}",
            serde_json::to_value(thinking)
                .expect("thinking level")
                .as_str()
                .unwrap()
        ));
    }
    if let Some(tools) = &preset.tools {
        parts.push(js_format!("tools:{}", tools.join(",")));
    }
    if let Some(instructions) = &preset.instructions
        && !instructions.is_empty()
    {
        let text = if instructions.len() > 30 {
            js_format!("{}...", instructions.slice(0, 27))
        } else {
            instructions.clone()
        };
        parts.push(js_format!("\"{}\"", text));
    }
    JsString::join(parts, " | ")
}
async fn select(pi: &ExtensionApi, shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let items = {
        let state = shared.lock().unwrap();
        state
            .presets
            .iter()
            .map(|(name, preset)| SelectItem {
                value: name.clone(),
                label: if state.active.as_ref() == Some(name) {
                    js_format!("{} (active)", name)
                } else {
                    name.clone()
                },
                description: Some(description(preset)),
            })
            .collect::<Vec<_>>()
    };
    if items.is_empty() {
        return no_presets(ctx);
    }
    let mut items = items;
    items.push(SelectItem::new("(none)", "(none)").with_description("Clear active preset, restore defaults"));
    let value = ctx
        .ui()?
        .custom(
            Arc::new(move |context, done| {
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Selector {
                    items: items.clone(),
                    selected: Mutex::new(0),
                    context,
                    done,
                }))
            }),
            None,
        )
        .await?;
    let Some(name) = value.as_ref().and_then(JsValue::as_string).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if name == "(none)" {
        return clear(pi, shared, ctx).await;
    }
    let preset = shared.lock().unwrap().presets.get(name).cloned();
    if let Some(preset) = preset {
        apply(pi, shared, name, preset, ctx).await?;
        ctx.ui()?
            .notify(&js_format!("Preset \"{}\" activated", name), Some(NotifyType::Info))?;
        status(shared, ctx)?;
    }
    Ok(())
}
// Rc-based TUI components are ephemeral on the UI thread; only data crosses
// ExtensionUiComponent's Send+Sync boundary. Selection state remains persistent.
struct Selector {
    items: Vec<SelectItem>,
    selected: Mutex<usize>,
    context: ExtensionUiFactoryContext,
    done: ExtensionUiDone,
}
impl Selector {
    fn list(&self) -> SelectList {
        let fg = |color| {
            let theme = self.context.theme.clone();
            Rc::new(move |text: &JsString| theme.fg(color, text)) as pi_tui::components::text::StyleFn
        };
        let mut list = SelectList::new(
            self.items.clone(),
            self.items.len().min(10),
            SelectListTheme {
                selected_prefix: fg(ThemeColor::Accent),
                selected_text: fg(ThemeColor::Accent),
                description: fg(ThemeColor::Muted),
                scroll_info: fg(ThemeColor::Dim),
                no_match: fg(ThemeColor::Warning),
            },
            SelectListLayoutOptions::default(),
        );
        list.set_selected_index(*self.selected.lock().unwrap() as isize);
        let done = self.done.clone();
        list.on_select = Some(Rc::new(move |item| done.call(Some(JsValue::from(json!(item.value))))));
        let done = self.done.clone();
        list.on_cancel = Some(Rc::new(move || done.call(None)));
        list
    }
    fn border(&self, width: usize) -> Vec<JsString> {
        let theme = self.context.theme.clone();
        DynamicBorder::new(Rc::new(move |s| theme.fg(ThemeColor::Accent, s))).render(width)
    }
}
impl ExtensionUiComponent for Selector {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let theme: &Theme = &self.context.theme;
        let mut lines = self.border(width);
        lines.extend(Text::new(theme.fg(ThemeColor::Accent, theme.bold("Select Preset")), 1, 1).render(width));
        lines.extend(self.list().render(width));
        lines.extend(
            Text::new(
                theme.fg(ThemeColor::Dim, "↑↓ navigate • enter select • esc cancel"),
                1,
                1,
            )
            .render(width),
        );
        lines.extend(self.border(width));
        lines.into_iter().collect::<Vec<_>>()
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let mut list = self.list();
        list.handle_input(data);
        *self.selected.lock().unwrap() = list.selected_index();
        self.context.tui.request_render();
    }
}
