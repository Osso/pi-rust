//! Opt-in native port of `overlay-qa-tests.ts`. Register the factory explicitly.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::tui::{OverlayAnchor as Anchor, OverlayMargin, SizeValue};
use pi_tui::{matches_key, visible_width};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

#[path = "overlay_qa_tests/overflow.rs"]
mod overflow;
#[path = "overlay_qa_tests/panels.rs"]
mod panels;
#[path = "overlay_qa_tests/render.rs"]
mod render;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Demo {
    Animation,
    Anchors,
    Margins,
    Stack,
    Overflow,
    Edge,
    Percent,
    MaxHeight,
    Sidepanel,
    Toggle,
    Passive,
    Focus,
    Streaming,
}
const COMMANDS: [(&str, &str, Demo); 13] = [
    (
        "overlay-animation",
        "Test real-time animation in overlay (~30 FPS)",
        Demo::Animation,
    ),
    ("overlay-anchors", "Cycle through all anchor positions", Demo::Anchors),
    ("overlay-margins", "Test margin and offset options", Demo::Margins),
    ("overlay-stack", "Test stacked overlays", Demo::Stack),
    (
        "overlay-overflow",
        "Test width overflow with streaming process output",
        Demo::Overflow,
    ),
    ("overlay-edge", "Test overlay positioned at terminal edge", Demo::Edge),
    ("overlay-percent", "Test percentage-based positioning", Demo::Percent),
    ("overlay-maxheight", "Test maxHeight truncation", Demo::MaxHeight),
    (
        "overlay-sidepanel",
        "Test responsive sidepanel (hides when terminal < 100 cols)",
        Demo::Sidepanel,
    ),
    (
        "overlay-toggle",
        "Test overlay toggle (press 't' to toggle visibility)",
        Demo::Toggle,
    ),
    (
        "overlay-passive",
        "Test non-capturing overlay (passive info panel alongside active overlay)",
        Demo::Passive,
    ),
    (
        "overlay-focus",
        "Test focus cycling, input routing, dismissal, and rendering order with overlays",
        Demo::Focus,
    ),
    (
        "overlay-streaming",
        "Multiple input panels with simulated streaming (Tab to cycle focus)",
        Demo::Streaming,
    ),
];
const ANCHORS: [(Anchor, &str); 9] = [
    (Anchor::TopLeft, "top-left"),
    (Anchor::TopCenter, "top-center"),
    (Anchor::TopRight, "top-right"),
    (Anchor::LeftCenter, "left-center"),
    (Anchor::Center, "center"),
    (Anchor::RightCenter, "right-center"),
    (Anchor::BottomLeft, "bottom-left"),
    (Anchor::BottomCenter, "bottom-center"),
    (Anchor::BottomRight, "bottom-right"),
];
const MARGINS: [&str; 5] = [
    "No margin (top-left)",
    "Margin: 3 all sides",
    "Margin: top=5, left=10",
    "Center + offset (10, -3)",
    "Bottom-right, margin: 2",
];
const PERCENT: [(&str, f64, f64); 5] = [
    ("rowPercent: 0 (top)", 0., 50.),
    ("rowPercent: 50 (middle)", 50., 50.),
    ("rowPercent: 100 (bottom)", 100., 50.),
    ("colPercent: 0 (left)", 50., 0.),
    ("colPercent: 100 (right)", 50., 100.),
];
fn absolute(n: f64) -> Option<SizeValue> {
    Some(SizeValue::Absolute(n))
}
fn all_margin(n: i64) -> OverlayMargin {
    OverlayMargin {
        top: Some(n),
        right: Some(n),
        bottom: Some(n),
        left: Some(n),
    }
}
fn overlay_options(demo: Demo, index: usize) -> ExtensionUiOverlayOptions {
    let mut o = ExtensionUiOverlayOptions {
        anchor: Some(Anchor::Center),
        width: absolute(50.),
        ..Default::default()
    };
    match demo {
        Demo::Animation => o.max_height = absolute(20.),
        Demo::Anchors => {
            o.anchor = Some(ANCHORS[index].0);
            o.width = absolute(40.);
        }
        Demo::Margins => {
            o.width = absolute(35.);
            o.anchor = Some(Anchor::TopLeft);
            match index {
                1 => o.margin = Some(all_margin(3)),
                2 => {
                    o.margin = Some(OverlayMargin {
                        top: Some(5),
                        left: Some(10),
                        ..Default::default()
                    })
                }
                3 => {
                    o.anchor = Some(Anchor::Center);
                    o.offset_x = Some(10);
                    o.offset_y = Some(-3);
                }
                4 => {
                    o.anchor = Some(Anchor::BottomRight);
                    o.margin = Some(all_margin(2));
                }
                _ => {}
            }
        }
        Demo::Stack => {
            o.offset_x = Some([-8, 0, 8][index]);
            o.offset_y = Some([-4, 0, 4][index]);
            o.max_height = absolute(15.);
        }
        Demo::Overflow => {
            o.width = absolute(90.);
            o.max_height = absolute(20.);
        }
        Demo::Edge => {
            o.anchor = Some(Anchor::RightCenter);
            o.width = absolute(40.);
            o.margin = Some(OverlayMargin {
                right: Some(0),
                ..Default::default()
            });
        }
        Demo::Percent => {
            o.anchor = None;
            o.width = absolute(30.);
            o.row = Some(SizeValue::Percent(PERCENT[index].1));
            o.col = Some(SizeValue::Percent(PERCENT[index].2));
        }
        Demo::MaxHeight => o.max_height = absolute(10.),
        Demo::Sidepanel => {
            o.anchor = Some(Anchor::RightCenter);
            o.width = Some(SizeValue::Percent(25.));
            o.min_width = Some(30);
            o.margin = Some(OverlayMargin {
                right: Some(1),
                ..Default::default()
            });
            o.visible = Some(Arc::new(|w, _| w >= 100));
        }
        Demo::Passive => o.width = absolute(48.),
        Demo::Focus | Demo::Streaming => {
            o.anchor = Some(Anchor::BottomCenter);
            o.width = absolute(if demo == Demo::Focus { 55. } else { 60. });
            o.margin = Some(OverlayMargin {
                bottom: Some(1),
                ..Default::default()
            });
        }
        Demo::Toggle => {}
    }
    o
}
async fn show(ui: &Arc<dyn ExtensionUiContext>, demo: Demo, index: usize) -> Result<Option<JsValue>, HandlerError> {
    ui.custom(
        Arc::new(move |ctx, done| {
            pi_coding_agent::core::extensions::ready_custom_component(Box::new(QaComponent(Model::new(
                demo, index, ctx, done,
            ))))
        }),
        Some(ExtensionUiCustomOptions {
            overlay: Some(true),
            overlay_options: Some(Arc::new(move || overlay_options(demo, index))),
            ..Default::default()
        }),
    )
    .await
    .map_err(HandlerError::from)
}
pub fn overlay_qa_tests_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    for (name, description, demo) in COMMANDS {
        api.register_command(
            name,
            Some(description.into()),
            None,
            Arc::new(move |_, ctx| {
                Box::pin(async move {
                    let ui = ctx.ui()?;
                    if matches!(demo, Demo::Passive | Demo::Focus | Demo::Streaming) {
                        ui.set_editor_text(&JsString::from(""))?;
                    }
                    if demo == Demo::Stack {
                        return stack(ui).await;
                    }
                    let count = match demo {
                        Demo::Anchors => 9,
                        Demo::Margins | Demo::Percent => 5,
                        _ => 1,
                    };
                    let mut index = 0;
                    loop {
                        let result = show(&ui, demo, index).await?;
                        if result.as_ref().and_then(JsValue::as_string) == Some(&JsString::from("next")) {
                            index = (index + 1) % count;
                            continue;
                        }
                        if demo == Demo::Anchors
                            && result.as_ref().and_then(JsValue::as_string) == Some(&JsString::from("confirm"))
                        {
                            ui.notify(
                                &JsString::from(format!("Selected: {}", ANCHORS[index].1)),
                                Some(NotifyType::Info),
                            )?;
                        }
                        break;
                    }
                    Ok(())
                })
            }),
        )?;
    }
    Ok(())
}
async fn stack(ui: Arc<dyn ExtensionUiContext>) -> Result<(), HandlerError> {
    ui.notify(&JsString::from("Showing overlay 1 (back)..."), Some(NotifyType::Info))?;
    let p1 = show(&ui, Demo::Stack, 0);
    tokio::pin!(p1);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    let first = tokio::select! {r=&mut p1=>Some(r),()=tokio::time::sleep_until(deadline)=>None};
    if first.is_some() {
        tokio::time::sleep_until(deadline).await;
    }
    ui.notify(&JsString::from("Showing overlay 2 (middle)..."), Some(NotifyType::Info))?;
    let p2 = show(&ui, Demo::Stack, 1);
    tokio::pin!(p2);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    let second = tokio::select! {r=&mut p2=>Some(r),()=tokio::time::sleep_until(deadline)=>None};
    if second.is_some() {
        tokio::time::sleep_until(deadline).await;
    }
    ui.notify(&JsString::from("Showing overlay 3 (front)..."), Some(NotifyType::Info))?;
    let p3 = show(&ui, Demo::Stack, 2);
    let (a, b, c) = tokio::join!(
        async {
            match first {
                Some(r) => r,
                None => p1.await,
            }
        },
        async {
            match second {
                Some(r) => r,
                None => p2.await,
            }
        },
        p3
    );
    let results = [a?, b?, c?];
    let results = JsString::join(
        results
            .iter()
            .map(|v| v.as_ref().and_then(JsValue::as_string).cloned().unwrap_or_default()),
        ", ",
    );
    ui.notify(&js_format!("Closed in order: {}", results), Some(NotifyType::Info))?;
    Ok(())
}

// The source has one global handle; all custom components run on the UI thread.
thread_local! {static TOGGLE_HANDLE:RefCell<Option<Rc<dyn ExtensionUiOverlayHandle>>>=const {RefCell::new(None)};}

struct Model {
    demo: Demo,
    index: usize,
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    state: RefCell<State>,
    focused: Cell<bool>,
    tasks: RefCell<Vec<tokio::task::JoinHandle<()>>>,
    entries: RefCell<Vec<panels::Entry>>,
    overflow: RefCell<Option<overflow::Process>>,
}
#[derive(Default)]
struct State {
    frame: usize,
    fps: usize,
    frames_since_fps: usize,
    last_fps: Option<tokio::time::Instant>,
    selected: usize,
    toggle_count: usize,
    toggling: bool,
    typed: Vec<u16>,
    inputs: usize,
    last_input: JsString,
    seconds: usize,
    focus_index: isize,
    stream_lines: Vec<String>,
    line_count: usize,
    closed: bool,
}
impl Model {
    fn new(demo: Demo, index: usize, ctx: ExtensionUiFactoryContext, done: ExtensionUiDone) -> Rc<Self> {
        let model = Rc::new(Self {
            demo,
            index,
            theme: ctx.theme,
            tui: ctx.tui,
            done,
            state: RefCell::new(State {
                focus_index: -1,
                last_fps: Some(tokio::time::Instant::now()),
                ..Default::default()
            }),
            focused: Cell::new(false),
            tasks: RefCell::new(Vec::new()),
            entries: RefCell::new(Vec::new()),
            overflow: RefCell::new(None),
        });
        match demo {
            Demo::Animation => model.start_timer(33),
            Demo::Passive => {
                panels::mount(&model);
                model.start_timer(1000);
            }
            Demo::Focus => panels::mount(&model),
            Demo::Streaming => {
                panels::mount(&model);
                model.start_timer(500);
            }
            Demo::Overflow => *model.overflow.borrow_mut() = Some(overflow::Process::start(&model)),
            _ => {}
        }
        model
    }
    fn start_timer(self: &Rc<Self>, ms: u64) {
        let weak = Rc::downgrade(self);
        self.tasks.borrow_mut().push(tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                let Some(model) = weak.upgrade() else { break };
                model.tick();
            }
        }));
    }
    fn tick(&self) {
        let mut s = self.state.borrow_mut();
        match self.demo {
            Demo::Animation => {
                s.frame += 1;
                s.frames_since_fps += 1;
                if s.last_fps.unwrap().elapsed() >= Duration::from_secs(1) {
                    s.fps = s.frames_since_fps;
                    s.frames_since_fps = 0;
                    s.last_fps = Some(tokio::time::Instant::now());
                }
            }
            Demo::Passive => s.seconds += 1,
            Demo::Streaming => {
                s.line_count += 1;
                let timestamp = pi_js_compat::date::to_locale_string(
                    pi_js_compat::date::js_now_millis(),
                    pi_js_compat::date::LocaleDateFormat::TimeEnUs,
                );
                let line = format!("[{timestamp}] Streaming line {}...", s.line_count);
                s.stream_lines.push(line);
                if s.stream_lines.len() > 8 {
                    s.stream_lines.remove(0);
                }
            }
            _ => {}
        }
        drop(s);
        self.tui.request_render();
    }
    fn stop_tasks(&self) {
        for task in self.tasks.borrow_mut().drain(..) {
            task.abort();
        }
    }
    fn cleanup(&self) {
        self.stop_tasks();
        for entry in self.entries.borrow_mut().drain(..) {
            if !entry.closed.replace(true) {
                entry.handle.hide();
            }
        }
        if let Some(process) = self.overflow.borrow_mut().as_mut() {
            process.dispose();
        }
    }
    fn close(&self) {
        if self.demo == Demo::Focus && self.state.borrow().closed {
            return;
        }
        self.state.borrow_mut().closed = true;
        self.cleanup();
        self.done.call(None);
    }
    fn input(self: &Rc<Self>, data: &JsString) {
        if self.demo == Demo::Passive {
            let mut s = self.state.borrow_mut();
            s.inputs += 1;
            s.last_input = JsString::from(format!(
                "len={} c0={}",
                data.encode_utf16().count(),
                data.encode_utf16()
                    .next()
                    .map(|u| u.to_string())
                    .unwrap_or("NaN".into())
            ));
        }
        if matches_key(data, "escape") || matches_key(data, "ctrl+c") {
            if self.demo == Demo::Focus {
                self.close();
                return;
            }
            let value = match self.demo {
                Demo::Anchors => Some(json!("cancel")),
                Demo::Margins | Demo::Percent => Some(json!("close")),
                Demo::Stack => Some(json!(format!("Overlay {}", self.index + 1))),
                _ => None,
            };
            self.cleanup();
            self.done.call(value.map(JsValue::from));
            return;
        }
        match self.demo {
            Demo::Anchors if matches_key(data, "return") => self.done.call(Some(JsValue::from(json!("confirm")))),
            Demo::Anchors | Demo::Margins | Demo::Percent
                if matches_key(data, "space") || matches_key(data, "right") =>
            {
                self.done.call(Some(JsValue::from(json!("next"))))
            }
            Demo::Stack if matches_key(data, "return") => self
                .done
                .call(Some(JsValue::from(json!(format!("Overlay {}", self.index + 1))))),
            Demo::Sidepanel => self.menu_input(data),
            Demo::Toggle if matches_key(data, "t") => self.toggle(),
            Demo::Passive => edit_text(&mut self.state.borrow_mut().typed, data),
            Demo::Focus if matches_key(data, "tab") => self.focus_first(),
            Demo::Streaming if matches_key(data, "tab") => self.cycle_streaming(),
            Demo::Overflow => {
                if let Some(p) = self.overflow.borrow().as_ref() {
                    p.input(data);
                    if matches_key(data, "up") || matches_key(data, "down") {
                        self.tui.request_render();
                    }
                }
            }
            _ => {}
        }
    }
    fn menu_input(&self, data: &JsString) {
        let mut s = self.state.borrow_mut();
        if matches_key(data, "up") {
            s.selected = s.selected.saturating_sub(1);
        } else if matches_key(data, "down") {
            s.selected = (s.selected + 1).min(4);
        } else if !matches_key(data, "return") {
            return;
        }
        drop(s);
        self.tui.request_render();
    }
    fn toggle(self: &Rc<Self>) {
        let handle = TOGGLE_HANDLE.with(|slot| slot.borrow().clone());
        let Some(handle) = handle.filter(|_| !self.state.borrow().toggling) else {
            return;
        };
        {
            let mut state = self.state.borrow_mut();
            state.toggling = true;
            state.toggle_count += 1;
        }
        handle.set_hidden(true);
        let model = self.clone();
        // Like the source timeout, this retains the component until restoration.
        tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(handle) = TOGGLE_HANDLE.with(|slot| slot.borrow().clone()) {
                handle.set_hidden(false);
                model.state.borrow_mut().toggling = false;
                model.tui.request_render();
            }
        });
    }
    fn focus_first(&self) {
        if let Some(e) = self.entries.borrow().iter().find(|e| !e.closed.get()) {
            e.handle.focus();
            self.tui.request_render();
        }
    }
    fn focus_next(&self, index: usize, direction: isize) {
        let entries = self.entries.borrow();
        let open = entries.iter().filter(|e| !e.closed.get()).collect::<Vec<_>>();
        let position = open
            .iter()
            .position(|e| e.index == index)
            .unwrap_or_else(|| panic!("Panel {} is not open", panels::LABELS[index]));
        let next = (position as isize + direction + open.len() as isize).rem_euclid(open.len() as isize) as usize;
        open[next].handle.focus();
        self.tui.request_render();
    }
    fn dismiss(&self, index: usize) {
        let entries = self.entries.borrow();
        let open = entries.iter().filter(|e| !e.closed.get()).collect::<Vec<_>>();
        let Some(position) = open.iter().position(|e| e.index == index) else {
            return;
        };
        open[position].closed.set(true);
        open[position].handle.hide();
        let remaining = open.into_iter().filter(|e| e.index != index).collect::<Vec<_>>();
        if remaining.is_empty() {
            drop(entries);
            self.close();
            return;
        }
        remaining[position % remaining.len()].handle.focus();
        self.tui.request_render();
    }
    fn cycle_streaming(&self) {
        let entries = self.entries.borrow();
        let mut s = self.state.borrow_mut();
        if s.focus_index >= 0 && (s.focus_index as usize) < entries.len() {
            entries[s.focus_index as usize].handle.unfocus();
        }
        s.focus_index += 1;
        if s.focus_index >= entries.len() as isize {
            s.focus_index = -1;
        }
        if s.focus_index >= 0 {
            entries[s.focus_index as usize].handle.focus();
        }
        drop(s);
        self.tui.request_render();
    }
}
fn edit_text(text: &mut Vec<u16>, data: &JsString) {
    if matches_key(data, "backspace") {
        text.pop();
    } else {
        let units = data.encode_utf16().collect::<Vec<_>>();
        if units.len() == 1 && units[0] >= 32 {
            text.extend(units);
        }
    }
}
struct QaComponent(Rc<Model>);
impl ExtensionUiComponent for QaComponent {
    fn focused(&self) -> Option<bool> {
        (self.0.demo == Demo::Passive).then(|| self.0.focused.get())
    }
    fn set_focused(&self, value: bool) {
        self.0.focused.set(value);
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        render::render(&self.0, width).into_iter().collect::<Vec<_>>()
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        self.0.input(data);
    }
    fn invalidate(&self) {}
    fn dispose(&self) {
        if self.0.demo == Demo::Streaming {
            self.0.close();
        } else {
            if self.0.demo == Demo::Focus {
                self.0.state.borrow_mut().closed = true;
            }
            self.0.cleanup();
        }
        if self.0.demo == Demo::Toggle {
            TOGGLE_HANDLE.with(|slot| slot.borrow_mut().take());
        }
    }
    fn on_overlay_handle(&self, handle: Rc<dyn ExtensionUiOverlayHandle>) {
        if self.0.demo == Demo::Toggle {
            TOGGLE_HANDLE.with(|slot| *slot.borrow_mut() = Some(handle));
        }
    }
}
