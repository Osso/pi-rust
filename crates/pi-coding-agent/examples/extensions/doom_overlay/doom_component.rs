//! Native half-block Doom overlay; external engine/module remains host-supplied.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use pi_coding_agent::core::extensions::{ExtensionUiComponent, ExtensionUiDone, ExtensionUiTui};
use pi_coding_agent::utils::uncaught::{ERROR, swallow_panics, throw_uncaught};
use pi_js_compat::js_json::JsString;
use tokio::task::JoinHandle;

use super::{doom_engine::DoomEngine, doom_keys::map_key_to_doom};

// Node setInterval truncates the source's 1000 / 35 milliseconds to 28.
const FRAME_DELAY: Duration = Duration::from_millis(28);
const FOOTER: &str = " DOOM | Q=Pause | WASD=Move | Shift+WASD=Run | Space=Use | F=Fire | 1-7=Weapons";

pub struct DoomOverlayComponent {
    engine: Arc<Mutex<DoomEngine>>,
    done: ExtensionUiDone,
    running: Rc<Cell<bool>>,
    timer: RefCell<Option<JoinHandle<()>>>,
}

impl DoomOverlayComponent {
    pub fn new(
        tui: ExtensionUiTui,
        engine: Arc<Mutex<DoomEngine>>,
        done: ExtensionUiDone,
        resume: bool,
    ) -> Result<Self, String> {
        if resume {
            pause(&mut engine.lock().unwrap())?;
        }
        let running = Rc::new(Cell::new(true));
        let timer = spawn_game_loop(tui, engine.clone(), done.clone(), running.clone());
        Ok(Self {
            engine,
            done,
            running,
            timer: RefCell::new(Some(timer)),
        })
    }

    fn send_input(&self, data: &JsString) -> Result<bool, String> {
        let released = pi_tui::keys::is_key_release(data);
        let mut engine = self.engine.lock().unwrap();
        if !released && (data == "q" || data == "Q") {
            pause(&mut engine)?;
            return Ok(true);
        }
        for key in map_key_to_doom(&JsString::from(data)) {
            engine.push_key(!released, key)?;
        }
        Ok(false)
    }
}

fn pause(engine: &mut DoomEngine) -> Result<(), String> {
    let key = super::doom_keys::DOOM_KEYS
        .iter()
        .find(|(name, _)| *name == "KEY_PAUSE")
        .unwrap()
        .1;
    engine.push_key(true, key)?;
    engine.push_key(false, key)
}

fn spawn_game_loop(
    tui: ExtensionUiTui,
    engine: Arc<Mutex<DoomEngine>>,
    done: ExtensionUiDone,
    running: Rc<Cell<bool>>,
) -> JoinHandle<()> {
    let start = tokio::time::Instant::now() + FRAME_DELAY;
    tokio::task::spawn_local(async move {
        let mut timer = tokio::time::interval_at(start, FRAME_DELAY);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            timer.tick().await;
            if !running.get() {
                return;
            }
            if !tick_and_render(&engine, &tui) {
                running.set(false);
                done.call(None);
                return;
            }
        }
    })
}

fn tick_and_render(engine: &Mutex<DoomEngine>, tui: &ExtensionUiTui) -> bool {
    if engine.lock().unwrap().tick().is_err() {
        return false;
    }
    let mut rendered = false;
    swallow_panics(|| {
        tui.request_render();
        rendered = true;
    });
    rendered
}

impl ExtensionUiComponent for DoomOverlayComponent {
    fn wants_key_release(&self) -> bool {
        true
    }

    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        // Throw only after releasing the engine lock: a TS throw does not poison it.
        let quit = self
            .send_input(data)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error));
        if quit {
            self.dispose();
            self.done.call(None);
        }
    }

    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let height = 10.max((width as f64 / 3.2).floor() as usize);
        let (rgba, source_width, source_height) = {
            let engine = self.engine.lock().unwrap();
            (engine.get_frame_rgba(), engine.width(), engine.height())
        };
        let rgba = rgba.unwrap_or_else(|error| throw_uncaught(ERROR, error));
        let mut lines = render_half_block(&rgba, source_width, source_height, width, height);
        lines.push(format!("\x1b[2m{}\x1b[0m", &FOOTER[..width.min(FOOTER.len())]));
        lines.into_iter().map(JsString::from).collect::<Vec<_>>()
    }

    fn invalidate(&self) {}

    fn dispose(&self) {
        self.running.set(false);
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.abort();
        }
    }
}

impl Drop for DoomOverlayComponent {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn render_half_block(rgba: &[u8], width: usize, height: usize, cols: usize, rows: usize) -> Vec<String> {
    let scale_x = width as f64 / cols as f64;
    let scale_y = height as f64 / (rows * 2) as f64;
    (0..rows)
        .map(|row| {
            let y1 = (row as f64 * 2.0 * scale_y).floor() as usize;
            let y2 = ((row * 2 + 1) as f64 * scale_y).floor() as usize;
            let mut line = String::new();
            for col in 0..cols {
                let x = (col as f64 * scale_x).floor() as usize;
                let top = color(rgba, (y1 * width + x) * 4);
                let bottom = color(rgba, (y2 * width + x) * 4);
                line.push_str(&format!(
                    "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
                    top[0], top[1], top[2], bottom[0], bottom[1], bottom[2]
                ));
            }
            line.push_str("\x1b[0m");
            line
        })
        .collect()
}

fn color(rgba: &[u8], offset: usize) -> [u8; 3] {
    std::array::from_fn(|channel| rgba.get(offset + channel).copied().unwrap_or(0))
}
