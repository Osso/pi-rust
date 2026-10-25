//! Opt-in native port of `border-status-editor.ts`.
use super::editor_composition::{ComposedEditor, EditorBehavior};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::components::custom_editor::{CustomEditor, CustomEditorOptions};
use pi_coding_agent::modes::interactive::theme::ThemeColor;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::number::{js_number_to_string, to_fixed};
use pi_tui::components::editor::EditorOptions;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{Component, visible_width};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
#[derive(Default)]
struct State {
    working: bool,
    index: usize,
    timer: Option<tokio::task::JoinHandle<()>>,
    tui: Option<ExtensionUiTui>,
}
impl State {
    fn stop(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
    }
    fn render(&self) {
        if let Some(tui) = &self.tui {
            tui.request_render();
        }
    }
}
fn fit_border(left: &JsString, right: &JsString, width: usize, border: &dyn Fn(&JsString) -> JsString) -> JsString {
    if width == 0 {
        return JsString::default();
    }
    if width == 1 {
        return border(&JsString::from("─"));
    }
    let (mut left, mut right) = (left.to_owned(), right.to_owned());
    while 2 + visible_width(&left) + visible_width(&right) + 3 > width && visible_width(&right) > 0 {
        right = truncate_to_width_with(&right, visible_width(&right) - 1, "", false);
    }
    while 2 + visible_width(&left) + visible_width(&right) + 3 > width && visible_width(&left) > 0 {
        left = truncate_to_width_with(&left, visible_width(&left) - 1, "", false);
    }
    let gap = width.saturating_sub(2 + visible_width(&left) + visible_width(&right));
    js_format!(
        "{}{}{}{}{}",
        border(&JsString::from("─")),
        left,
        border(&JsString::from(&"─".repeat(gap))),
        right,
        border(&JsString::from("─"))
    )
}
fn format_context(ctx: &ExtensionContext) -> String {
    let usage = ctx.get_context_usage().expect("live extension context");
    let window = usage
        .as_ref()
        .map(|u| u.context_window)
        .or_else(|| ctx.model().expect("live model").map(|m| m.context_window));
    match (window, usage.and_then(|u| u.percent)) {
        (Some(w), Some(percent)) if w != 0.0 => format!(
            "ctx {}%/{}k",
            js_number_to_string((percent + 0.5).floor()),
            to_fixed(w / 1000.0, 0)
        ),
        _ => "ctx ?".to_owned(),
    }
}
struct BorderStatus {
    ctx: ExtensionContext,
    api: ExtensionApi,
    state: Arc<Mutex<State>>,
    branch: Arc<Mutex<Option<String>>>,
}
impl EditorBehavior for BorderStatus {
    fn render(&mut self, editor: &mut CustomEditor, width: usize) -> Vec<JsString> {
        let mut lines = editor.render(width);
        if lines.len() < 2 {
            return lines;
        }
        let theme = self
            .ctx
            .ui()
            .expect("live UI")
            .theme()
            .unwrap_or_else(|error| pi_js_compat::callback_error::throw_callback_uncaught(error));
        let model = self
            .ctx
            .model()
            .expect("live model")
            .map(|m| format!("{}/{}", m.provider, m.id))
            .unwrap_or_else(|| "no model".to_owned());
        let thinking = serde_json::to_value(self.api.get_thinking_level().expect("live thinking level"))
            .expect("thinking level serializes");
        let state = self.state.lock().expect("spinner state");
        let top = if state.working {
            theme.fg(ThemeColor::Accent, format!(" {} ", FRAMES[state.index]))
        } else {
            JsString::default()
        };
        let bottom_left = theme.fg(
            ThemeColor::Muted,
            format!(" {model} · {} ", thinking.as_str().expect("thinking level is a string")),
        );
        let cwd = self.ctx.cwd().expect("live cwd");
        let cwd = match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && cwd.starts_with(&home) => {
                js_format!("~{}", cwd.slice(home.len(), cwd.len()))
            }
            _ => cwd,
        };
        let branch = self.branch.lock().expect("branch state");
        let branch = branch.as_ref().map(|b| format!(" ({b})")).unwrap_or_default();
        let bottom_right = theme.fg(
            ThemeColor::Muted,
            &js_format!(" {} · {}{} ", format_context(&self.ctx), cwd, branch),
        );
        let border = editor.get_border_color();
        lines[0] = fit_border(&top, &JsString::from(""), width, border.as_ref());
        let last = lines.len() - 1;
        lines[last] = fit_border(&bottom_left, &bottom_right, width, border.as_ref());
        lines
    }
}
struct EmptyFooter;
impl ExtensionUiComponent for EmptyFooter {
    fn render(&self, _: usize) -> Vec<pi_js_compat::js_json::JsString> {
        Vec::new()
    }
}

pub fn border_status_editor_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state = Arc::new(Mutex::new(State::default()));
    let started = state.clone();
    pi.on_agent_start(
        move |_, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = started.clone();
            Box::pin(async move {
                let mut current = state.lock().expect("spinner state");
                current.working = true;
                current.stop();
                let ticks = state.clone();
                current.timer = Some(tokio::spawn(async move {
                    let period = Duration::from_millis(80);
                    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                    loop {
                        interval.tick().await;
                        let mut state = ticks.lock().expect("spinner state");
                        state.index = (state.index + 1) % FRAMES.len();
                        state.render();
                    }
                }));
                current.render();
                Ok(None)
            })
        },
    )?;
    let ended = state.clone();
    pi.on_agent_end(
        move |_, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = ended.clone();
            Box::pin(async move {
                let mut state = state.lock().expect("spinner state");
                state.working = false;
                state.stop();
                state.render();
                Ok(None)
            })
        },
    )?;
    let shutdown = state.clone();
    pi.on_session_shutdown(
        move |_, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = shutdown.clone();
            Box::pin(async move {
                let mut state = state.lock().expect("spinner state");
                state.stop();
                state.tui = None;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    pi.on_session_start(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            let api = api.clone();
            Box::pin(async move {
                let ui = ctx.ui()?;
                ui.set_working_visible(false);
                ui.set_footer(Some(Arc::new(|_| Box::new(EmptyFooter))));
                let branch = Arc::new(Mutex::new(None));
                let refreshed = branch.clone();
                let refresh_api = api.clone();
                let refresh_state = state.clone();
                let cwd = ctx.cwd()?;
                tokio::spawn(async move {
                    // The pinned example deliberately ignores git failures (non-repositories have no branch).
                    let result = refresh_api
                        .exec(
                            "git".to_owned(),
                            vec!["branch".to_owned(), "--show-current".to_owned()],
                            Some(ExtensionExecOptions {
                                cwd: Some(cwd),
                                ..Default::default()
                            }),
                        )
                        .await
                        .ok();
                    let name = result
                        .map(|r| pi_js_compat::string::js_trim(&r.stdout).to_owned())
                        .filter(|name| !name.is_empty());
                    *refreshed.lock().expect("branch state") = name;
                    refresh_state.lock().expect("spinner state").render();
                });
                ui.set_editor_component(Some(Arc::new(move |tui, theme, kb| {
                    state.lock().expect("spinner state").tui = Some(tui.clone());
                    Rc::new(RefCell::new(ComposedEditor {
                        editor: CustomEditor::new(
                            Rc::new(tui),
                            theme,
                            kb,
                            CustomEditorOptions {
                                editor: EditorOptions {
                                    padding_x: Some(0),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        )
                        .expect("default editor has no history DB"),
                        behavior: BorderStatus {
                            ctx: ctx.clone(),
                            api: api.clone(),
                            state: state.clone(),
                            branch: branch.clone(),
                        },
                    }))
                })));
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
