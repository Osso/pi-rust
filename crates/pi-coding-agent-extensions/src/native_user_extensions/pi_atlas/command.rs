//! Hidden native registration and the two custom-UI phases of /atlas.
use super::{
    cache, compute,
    data::{Session, StatsSummary},
    format, loading,
    state::DashboardState,
    ui::{self, View},
};
use crate::core::extensions::{
    ExtensionApi, ExtensionCommandContext, ExtensionUiComponent, ExtensionUiCustomOptions, ExtensionUiDone,
    ExtensionUiOverlayOptions, ExtensionUiTui, HandlerError, NotifyType, ready_custom_component,
};
use crate::modes::interactive::theme::Theme;
use pi_js_compat::{
    js_format,
    js_json::{self, JsString, JsValue},
};
use pi_tui::keys::matches_key;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Paths {
    cache: PathBuf,
    sessions: PathBuf,
}

pub fn extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    let base = match pi_js_compat::process_env::get_native("XDG_CONFIG_HOME") {
        Some(base) => base,
        None => crate::utils::paths::node_join(
            pi_js_compat::node_os::homedir().map_err(|error| HandlerError::new(error.message.clone()))?,
            ".config",
        ),
    };
    let config = crate::utils::paths::node_join(base, "pi");
    let path = |suffix: &str| {
        PathBuf::from(js_json::encode_node_utf8(&crate::utils::paths::node_join(
            config.clone(),
            suffix,
        )))
    };
    let paths = Paths {
        cache: path("pi-atlas-cache.json"),
        sessions: path("agent/sessions"),
    };
    api.register_command(
        "atlas",
        Some("Show Pi Atlas usage dashboard".into()),
        None,
        Arc::new(move |_args, ctx| {
            let paths = paths.clone();
            Box::pin(async move { run_atlas(ctx, paths).await })
        }),
    )
    .map_err(HandlerError::from)
}

fn overlay_options() -> ExtensionUiCustomOptions {
    ExtensionUiCustomOptions {
        overlay: Some(true),
        overlay_options: Some(Arc::new(|| ExtensionUiOverlayOptions {
            min_width: Some(100),
            width: Some(pi_tui::tui::SizeValue::Percent(50.0)),
            max_height: Some(pi_tui::tui::SizeValue::Percent(80.0)),
            anchor: Some(pi_tui::tui::OverlayAnchor::TopCenter),
            margin: Some(pi_tui::tui::OverlayMargin {
                top: Some(2),
                right: Some(2),
                bottom: Some(2),
                left: Some(2),
            }),
            ..Default::default()
        })),
        ..Default::default()
    }
}

async fn run_atlas(ctx: ExtensionCommandContext, paths: Paths) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if !ctx.has_ui()? {
        ui.notify(
            &"Stats dashboard requires interactive mode".into(),
            Some(NotifyType::Error),
        )?;
        return Ok(());
    }
    let timestamp = cache::read_cache(&paths.cache).and_then(|cache| cache["generatedAt"].as_string().cloned());
    let update = timestamp.filter(|timestamp| !timestamp.is_empty()).map(|timestamp| {
        js_format!(
            "Last update : {}",
            format::format_cache_timestamp(
                &timestamp,
                pi_js_compat::date::js_now_millis(),
                &jiff::tz::TimeZone::system()
            )
        )
    });
    let loaded = ui
        .custom(
            Arc::new(move |context, done| {
                let progress = Rc::new(RefCell::new(cache::LoadingProgress::default()));
                start_loading(paths.clone(), progress.clone(), context.tui.clone(), done.clone());
                ready_custom_component(Box::new(Loading {
                    progress,
                    theme: context.theme,
                    done,
                }))
            }),
            Some(overlay_options()),
        )
        .await;
    let values = match loaded {
        Ok(Some(JsValue::Array(values))) => values,
        Ok(None) => return Ok(()),
        Ok(Some(_)) => return Err(HandlerError::new("Invalid atlas loading result")),
        Err(error) => {
            ui.notify(&"Failed to parse session logs".into(), Some(NotifyType::Error))?;
            ui.notify(&error.message, Some(NotifyType::Error))?;
            return Ok(());
        }
    };
    let sessions = values
        .iter()
        .map(js_json::from_value::<Session>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let mut summaries = Vec::new();
    for range in ui::RANGES {
        summaries.push(
            compute::summarize_stats(
                &sessions,
                &range.into(),
                &JsValue::Null,
                pi_js_compat::date::js_now_millis(),
            )
            .map_err(HandlerError::from)?,
        );
    }
    let summaries: [StatsSummary; 4] = summaries
        .try_into()
        .map_err(|_| HandlerError::new("Invalid atlas range count"))?;
    ui.custom(
        Arc::new(move |context, done| {
            let rows = context.tui.terminal_rows();
            ready_custom_component(Box::new(Dashboard {
                summaries: summaries.clone(),
                update: update.clone(),
                theme: context.theme,
                tui: context.tui,
                done,
                state: RefCell::new(DashboardState::new(rows)),
                cache: RefCell::new(None),
                marquee_start: Cell::new(None),
                timer: RefCell::new(None),
            }))
        }),
        Some(overlay_options()),
    )
    .await?;
    Ok(())
}

fn start_loading(
    paths: Paths,
    progress: Rc<RefCell<cache::LoadingProgress>>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
) {
    let (sender, mut receiver) = tokio::sync::watch::channel(cache::LoadingProgress::default());
    let mut worker = tokio::task::spawn_blocking(move || {
        cache::load_aggregate_with_progress(
            &paths.cache,
            &paths.sessions,
            &pi_js_compat::date::js_now_millis,
            false,
            &mut |progress| {
                sender.send_replace(progress);
            },
            &mut |message| {
                use std::io::Write;
                let line = js_json::encode_node_utf8(&(message + "\n"));
                std::io::stderr()
                    .write_all(line.as_bytes())
                    .map_err(|error| JsString::from(error.to_string()))
            },
        )
    });
    tokio::task::spawn_local(async move {
        loop {
            tokio::select! {
                result = &mut worker => {
                    let values = match result { Ok(Ok(values)) => values, Ok(Err(_)) | Err(_) => Vec::new() };
                    done.call(Some(JsValue::Array(values)));
                    break;
                },
                result = receiver.changed() => {
                    if result.is_ok() {
                        *progress.borrow_mut() = receiver.borrow_and_update().clone();
                        tui.request_render();
                    } else {
                        let values = match worker.await { Ok(Ok(values)) => values, Ok(Err(_)) | Err(_) => Vec::new() };
                        done.call(Some(JsValue::Array(values)));
                        break;
                    }
                }
            }
        }
    });
}

struct Loading {
    progress: Rc<RefCell<cache::LoadingProgress>>,
    theme: Arc<Theme>,
    done: ExtensionUiDone,
}
impl ExtensionUiComponent for Loading {
    fn render(&self, width: usize) -> Vec<JsString> {
        loading::render(&self.progress.borrow(), width, self.theme.as_ref())
    }
    fn handle_input(&self, data: &JsString) {
        if close_key(data) {
            self.done.call(None);
        }
    }
}

struct Dashboard {
    summaries: [StatsSummary; 4],
    update: Option<JsString>,
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    state: RefCell<DashboardState>,
    cache: RefCell<Option<(usize, Vec<JsString>)>>,
    marquee_start: Cell<Option<Instant>>,
    timer: RefCell<Option<tokio::task::JoinHandle<()>>>,
}
impl Dashboard {
    fn stop_timer(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.abort();
        }
        self.marquee_start.set(None);
    }
    fn start_timer(&self) {
        if self.timer.borrow().is_some() {
            return;
        }
        self.marquee_start.set(Some(Instant::now()));
        let tui = self.tui.clone();
        *self.timer.borrow_mut() = Some(tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                tui.request_render();
            }
        }));
    }
}
impl ExtensionUiComponent for Dashboard {
    fn render(&self, width: usize) -> Vec<JsString> {
        if let Some((cached_width, lines)) = self.cache.borrow().as_ref()
            && *cached_width == width
        {
            if self.state.borrow_mut().resize(self.tui.terminal_rows()) {
                self.stop_timer();
            }
            return lines.clone();
        }
        let mut state = self.state.borrow_mut();
        let marquee = Cell::new(false);
        let tick = self
            .marquee_start
            .get()
            .map_or(0, |started| (started.elapsed().as_millis() / 150) as usize);
        let lines = View {
            summary: &self.summaries[state.range],
            tab: state.tab,
            range: state.range,
            focus: state.focus[state.tab],
            scroll: state.scroll[state.tab],
            marquee_tick: tick,
            marquee: &marquee,
            rows: state.rows,
            all_empty: self.summaries.iter().all(|summary| summary.session_count == 0.0),
            update: self.update.as_ref(),
            paint: self.theme.as_ref(),
        }
        .render(width);
        if marquee.get() {
            self.start_timer();
        }
        let range = state.range;
        state.mark_rendered(&self.summaries[range]);
        if state.resize(self.tui.terminal_rows()) {
            self.stop_timer();
        }
        *self.cache.borrow_mut() = Some((width, lines.clone()));
        lines
    }
    fn handle_input(&self, data: &JsString) {
        self.invalidate();
        if self.state.borrow_mut().input(data, &self.summaries) {
            self.done.call(None);
        }
        self.tui.request_render();
    }
    fn invalidate(&self) {
        *self.cache.borrow_mut() = None;
        self.stop_timer();
    }
    fn dispose(&self) {
        self.stop_timer();
    }
}

fn close_key(data: &JsString) -> bool {
    matches_key(data, "escape") || data == "q" || data == "Q"
}

#[cfg(test)]
mod tests {
    use crate::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, ResourceLoader};
    use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
    use pi_js_compat::js_json::JsString;
    use std::{
        fs,
        sync::{Arc, Mutex},
    };

    #[tokio::test]
    async fn settings_package_loads_atlas_native_command() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join(".config/pi/agent");
        let package = dir.path().join("Repos/pi-atlas");
        fs::create_dir_all(&agent).unwrap();
        fs::create_dir_all(package.join("src")).unwrap();
        fs::write(
            agent.join("settings.json"),
            r#"{"packages":["../../../Repos/pi-atlas"]}"#,
        )
        .unwrap();
        fs::write(
            package.join("package.json"),
            r#"{"name":"@mohndoe/pi-atlas","pi":{"extensions":["./src/index.ts"]}}"#,
        )
        .unwrap();
        fs::write(package.join("src/index.ts"), "").unwrap();
        let cwd = JsString::from(dir.path().to_str().unwrap());
        let agent_dir = JsString::from(agent.to_str().unwrap());
        let settings = SettingsManager::create(
            &cwd,
            Some(&agent_dir),
            SettingsManagerCreateOptions {
                project_trusted: Some(true),
            },
        )
        .unwrap();
        let mut registry = crate::core::extensions::ExtensionRegistry::new();
        crate::native_user_extensions::register(&mut registry);
        let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
            cwd,
            agent_dir,
            settings_manager: Some(Arc::new(Mutex::new(settings))),
            extension_registry: Some(Arc::new(registry)),
            ..Default::default()
        })
        .unwrap();
        loader.reload().await.unwrap();
        let result = loader.get_extensions();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.extensions.len(), 1);
        let extension = result.extensions[0].read();
        let command = extension.commands.get(&JsString::from("atlas")).expect("atlas command");
        assert_eq!(command.description, Some("Show Pi Atlas usage dashboard".into()));
        assert!(extension.handlers.is_empty());
        assert!(extension.tools.is_empty());
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
