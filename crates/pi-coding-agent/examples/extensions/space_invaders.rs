//! Opt-in native port of `space-invaders.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{
    keys::{is_key_release, matches_key},
    utils::{strip_terminal_sequences, visible_width},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
const WIDTH: i32 = 60;
const HEIGHT: i32 = 24;
const PLAYER_Y: i32 = HEIGHT - 2;
const TICK: Duration = Duration::from_millis(50);
type Random = Arc<dyn Fn() -> f64 + Send + Sync>;
type Save = Arc<dyn Fn(Option<State>) + Send + Sync>;
#[derive(Clone, Deserialize, Serialize)]
struct Player {
    x: i32,
    lives: i32,
}
#[derive(Clone, Deserialize, Serialize)]
struct Alien {
    x: i32,
    y: i32,
    r#type: usize,
    alive: bool,
}
#[derive(Clone, Deserialize, Serialize)]
struct Bullet {
    x: i32,
    y: i32,
    direction: i32,
}
#[derive(Clone, Deserialize, Serialize)]
struct Shield {
    x: i32,
    segments: Vec<Vec<bool>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct State {
    player: Player,
    aliens: Vec<Alien>,
    alien_direction: i32,
    alien_move_counter: i32,
    alien_move_delay: i32,
    alien_dropping: bool,
    bullets: Vec<Bullet>,
    shields: Vec<Shield>,
    score: i64,
    high_score: i64,
    level: i32,
    game_over: bool,
    victory: bool,
    alien_shoot_counter: i32,
}
fn initial(high_score: i64, level: i32) -> State {
    State {
        player: Player { x: WIDTH / 2, lives: 3 },
        aliens: (0..5)
            .flat_map(|row| {
                (0..11).map(move |col| Alien {
                    x: 4 + col * 5,
                    y: 2 + row * 2,
                    r#type: if row == 0 {
                        2
                    } else if row < 3 {
                        1
                    } else {
                        0
                    },
                    alive: true,
                })
            })
            .collect(),
        alien_direction: 1,
        alien_move_counter: 0,
        alien_move_delay: 5.max(20 - level * 2),
        alien_dropping: false,
        bullets: Vec::new(),
        shields: [8, 22, 36, 50]
            .into_iter()
            .map(|x| Shield {
                x,
                segments: vec![vec![true; 4], vec![true; 4], vec![true, false, false, true]],
            })
            .collect(),
        score: 0,
        high_score,
        level,
        game_over: false,
        victory: false,
        alien_shoot_counter: 0,
    }
}
fn random_unit() -> f64 {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).expect("OS randomness for Space Invaders");
    (u64::from_ne_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}
pub fn space_invaders_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    space_invaders_extension_with_random(api, Arc::new(random_unit))
}
pub(crate) fn space_invaders_extension_with_random(api: &ExtensionApi, random: Random) -> Result<(), HandlerError> {
    let pi = api.clone();
    api.register_command(
        "invaders",
        Some("Play Space Invaders!".into()),
        None,
        Arc::new(move |_, ctx| {
            let pi = pi.clone();
            let random = random.clone();
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?.notify(
                        &JsString::from("Space Invaders requires interactive mode"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(());
                }
                let entries = ctx.session_manager()?.get_entries();
                let entries = serde_json::to_value(entries).map_err(|e| HandlerError::new(e.to_string()))?;
                let saved = entries
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|e| e["type"] == "custom" && e["customType"] == "space-invaders-save")
                    .map(|e| e["data"].clone())
                    .filter(|v| !v.is_null());
                let saved: Option<State> = saved
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| HandlerError::new(e.to_string()))?;
                let error = Arc::new(Mutex::new(None));
                let captured = error.clone();
                let save: Save = Arc::new(move |state| {
                    if let Err(e) = pi.append_entry(
                        &JsString::from("space-invaders-save"),
                        Some(JsValue::from(state.map_or(Value::Null, |s| json!(s)))),
                    ) {
                        *captured.lock().unwrap() = Some(e);
                    }
                });
                ctx.ui()?
                    .custom(
                        Arc::new(move |context, done| {
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(Invaders::new(
                                context.tui,
                                done,
                                save.clone(),
                                saved.clone(),
                                random.clone(),
                            )))
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
#[derive(Default)]
struct Keys {
    left: bool,
    right: bool,
    fire: bool,
}
struct Inner {
    state: State,
    keys: Keys,
    paused: bool,
    fire_cooldown: i32,
    player_move_counter: i32,
    version: u64,
    cached_version: Option<u64>,
    cached_width: usize,
    cached_lines: Vec<String>,
}
impl Inner {
    fn tick(&mut self, random: &Random) {
        self.player_move_counter += 1;
        if self.player_move_counter >= 2 {
            self.player_move_counter = 0;
            if self.keys.left && self.state.player.x > 2 {
                self.state.player.x -= 1;
            }
            if self.keys.right && self.state.player.x < WIDTH - 3 {
                self.state.player.x += 1;
            }
        }
        if self.fire_cooldown > 0 {
            self.fire_cooldown -= 1;
        }
        if self.keys.fire
            && self.fire_cooldown == 0
            && self.state.bullets.iter().filter(|b| b.direction == -1).count() < 2
        {
            self.state.bullets.push(Bullet {
                x: self.state.player.x,
                y: PLAYER_Y - 1,
                direction: -1,
            });
            self.fire_cooldown = 8;
        }
        let s = &mut self.state;
        for bullet in &mut s.bullets {
            bullet.y += bullet.direction;
        }
        s.bullets.retain(|b| b.y >= 0 && b.y < HEIGHT);
        s.alien_move_counter += 1;
        if s.alien_move_counter >= s.alien_move_delay {
            s.alien_move_counter = 0;
            move_aliens(s);
        }
        s.alien_shoot_counter += 1;
        if s.alien_shoot_counter >= 30 {
            s.alien_shoot_counter = 0;
            alien_shoot(s, random);
        }
        check_collisions(s);
        if s.aliens.iter().all(|a| !a.alive) {
            s.victory = true;
        }
    }
}
fn move_aliens(s: &mut State) {
    let count = s.aliens.iter().filter(|a| a.alive).count();
    if count == 0 {
        return;
    }
    if s.alien_dropping {
        for alien in s.aliens.iter_mut().filter(|a| a.alive) {
            alien.y += 1;
            if alien.y >= PLAYER_Y - 1 {
                s.game_over = true;
                return;
            }
        }
        s.alien_dropping = false;
    } else {
        let min = s.aliens.iter().filter(|a| a.alive).map(|a| a.x).min().unwrap();
        let max = s.aliens.iter().filter(|a| a.alive).map(|a| a.x).max().unwrap();
        if (s.alien_direction == 1 && max >= WIDTH - 3) || (s.alien_direction == -1 && min <= 2) {
            s.alien_direction *= -1;
            s.alien_dropping = true;
        } else {
            for alien in s.aliens.iter_mut().filter(|a| a.alive) {
                alien.x += s.alien_direction;
            }
        }
    }
    if count <= 5 {
        s.alien_move_delay = 1;
    } else if count <= 10 {
        s.alien_move_delay = 2;
    } else if count <= 20 {
        s.alien_move_delay = 3;
    }
}
fn alien_shoot(s: &mut State, random: &Random) {
    // Vec preserves Map insertion order even when replacing a column's shooter.
    let mut columns: Vec<&Alien> = Vec::new();
    for alien in s.aliens.iter().filter(|a| a.alive) {
        if let Some(existing) = columns.iter_mut().find(|a| a.x == alien.x) {
            if alien.y > existing.y {
                *existing = alien;
            }
        } else {
            columns.push(alien);
        }
    }
    if !columns.is_empty() && s.bullets.iter().filter(|b| b.direction == 1).count() < 3 {
        let shooter = columns[(random() * columns.len() as f64).floor() as usize];
        s.bullets.push(Bullet {
            x: shooter.x,
            y: shooter.y + 1,
            direction: 1,
        });
    }
}
fn check_collisions(s: &mut State) {
    s.bullets.retain(|bullet| {
        let mut remove = false;
        if bullet.direction == -1
            && let Some(alien) = s
                .aliens
                .iter_mut()
                .find(|a| a.alive && (bullet.x - a.x).abs() <= 1 && bullet.y == a.y)
        {
            alien.alive = false;
            remove = true;
            s.score += [10, 20, 30][alien.r#type];
            s.high_score = s.high_score.max(s.score);
        }
        if bullet.direction == 1 && (bullet.x - s.player.x).abs() <= 1 && bullet.y == PLAYER_Y {
            remove = true;
            s.player.lives -= 1;
            if s.player.lives <= 0 {
                s.game_over = true;
            }
        }
        for shield in &mut s.shields {
            let x = bullet.x - shield.x;
            let y = bullet.y - (PLAYER_Y - 5);
            if (0..4).contains(&x) && (0..3).contains(&y) && shield.segments[y as usize][x as usize] {
                shield.segments[y as usize][x as usize] = false;
                remove = true;
            }
        }
        !remove
    });
}
struct Invaders {
    inner: Arc<Mutex<Inner>>,
    timer: Mutex<Option<JoinHandle<()>>>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    save: Save,
    random: Random,
}
impl Invaders {
    fn new(tui: ExtensionUiTui, done: ExtensionUiDone, save: Save, saved: Option<State>, random: Random) -> Self {
        let paused = saved.as_ref().is_some_and(|s| !s.game_over && !s.victory);
        let state = if paused {
            saved.unwrap()
        } else {
            initial(saved.map_or(0, |s| s.high_score), 1)
        };
        let component = Self {
            inner: Arc::new(Mutex::new(Inner {
                state,
                keys: Keys::default(),
                paused,
                fire_cooldown: 0,
                player_move_counter: 0,
                version: 0,
                cached_version: None,
                cached_width: 0,
                cached_lines: Vec::new(),
            })),
            timer: Mutex::new(None),
            tui,
            done,
            save,
            random,
        };
        if !paused {
            component.start_game();
        }
        component
    }
    fn start_game(&self) {
        let inner = self.inner.clone();
        let tui = self.tui.clone();
        let random = self.random.clone();
        let start = tokio::time::Instant::now() + TICK;
        let task = tokio::spawn(async move {
            let mut timer = tokio::time::interval_at(start, TICK);
            loop {
                timer.tick().await;
                let mut inner = inner.lock().unwrap();
                if !inner.state.game_over && !inner.state.victory {
                    inner.tick(&random);
                    inner.version += 1;
                    tui.request_render();
                }
            }
        });
        *self.timer.lock().unwrap() = Some(task);
    }
    fn stop(&self) {
        if let Some(timer) = self.timer.lock().unwrap().take() {
            timer.abort();
        }
    }
}
impl Drop for Invaders {
    fn drop(&mut self) {
        self.stop();
    }
}
impl ExtensionUiComponent for Invaders {
    fn wants_key_release(&self) -> bool {
        true
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let released = is_key_release(data);
        let mut inner = self.inner.lock().unwrap();
        if inner.paused && !released {
            if matches_key(data, "escape") || (data == "q" || data == "Q") {
                self.stop();
                drop(inner);
                self.done.call(None);
                return;
            }
            inner.paused = false;
            drop(inner);
            self.start_game();
            return;
        }
        if !released && matches_key(data, "escape") {
            self.stop();
            let state = inner.state.clone();
            drop(inner);
            (self.save)(Some(state));
            self.done.call(None);
            return;
        }
        if !released && (data == "q" || data == "Q") {
            self.stop();
            drop(inner);
            (self.save)(None);
            self.done.call(None);
            return;
        }
        if matches_key(data, "left") || (data == "a" || data == "A") || matches_key(data, "a") {
            inner.keys.left = !released;
        }
        if matches_key(data, "right") || (data == "d" || data == "D") || matches_key(data, "d") {
            inner.keys.right = !released;
        }
        if matches_key(data, "space") || (data == " " || data == "f" || data == "F") || matches_key(data, "f") {
            inner.keys.fire = !released;
        }
        if !released && (inner.state.game_over || inner.state.victory) && (data == "r" || data == "R" || data == " ") {
            let level = if inner.state.victory { inner.state.level + 1 } else { 1 };
            inner.state = initial(inner.state.high_score, level);
            inner.keys = Keys::default();
            inner.version += 1;
            drop(inner);
            (self.save)(None);
            self.tui.request_render();
        }
    }
    fn invalidate(&self) {
        self.inner.lock().unwrap().cached_width = 0;
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut inner = self.inner.lock().unwrap();
        if width == inner.cached_width && inner.cached_version == Some(inner.version) {
            return inner
                .cached_lines
                .clone()
                .into_iter()
                .map(JsString::from)
                .collect::<Vec<_>>();
        }
        if inner.state.player.lives < 0 {
            let lives = inner.state.player.lives;
            drop(inner);
            pi_coding_agent::utils::uncaught::throw_uncaught("RangeError", format!("Invalid count value: {lives}"));
        }
        let lines = render_frame(&inner, width);
        inner.cached_width = width;
        inner.cached_version = Some(inner.version);
        inner.cached_lines = lines.clone();
        lines.into_iter().map(JsString::from).collect::<Vec<_>>()
    }
    fn dispose(&self) {
        self.stop();
    }
}
fn color(code: u8, s: impl AsRef<str>) -> String {
    format!("\x1b[{code}m{}\x1b[0m", s.as_ref())
}
fn dim(s: impl AsRef<str>) -> String {
    format!("\x1b[2m{}\x1b[22m", s.as_ref())
}
fn bold(s: impl AsRef<str>) -> String {
    format!("\x1b[1m{}\x1b[22m", s.as_ref())
}
fn box_line(content: &str) -> String {
    format!(
        "{}{content}{}{}",
        dim(" │"),
        " ".repeat((WIDTH as usize).saturating_sub(visible_width(content))),
        dim("│")
    )
}
fn cell(s: &State, x: i32, y: i32) -> String {
    if let Some(a) = s.aliens.iter().find(|a| a.alive && a.y == y && (a.x - x).abs() <= 1) {
        let sprites = [["╲", "▼", "╱"], ["╱", "◆", "╲"], ["◄", "☆", "►"]];
        return color([32, 36, 35][a.r#type], sprites[a.r#type][(x - a.x + 1) as usize]);
    }
    for shield in &s.shields {
        let rx = x - shield.x;
        let ry = y - (PLAYER_Y - 5);
        if (0..4).contains(&rx) && (0..3).contains(&ry) {
            if shield.segments[ry as usize][rx as usize] {
                return dim("█");
            }
            break;
        }
    }
    if y == PLAYER_Y && (x - s.player.x).abs() <= 1 {
        return color(97, if x == s.player.x { "▲" } else { "═" });
    }
    if let Some(b) = s.bullets.iter().find(|b| b.x == x && b.y == y) {
        return color(if b.direction == -1 { 33 } else { 31 }, "│");
    }
    " ".into()
}
fn render_frame(inner: &Inner, width: usize) -> Vec<String> {
    let s = &inner.state;
    let lives = s.player.lives as usize;
    let header = format!(
        "{} │ Score: {} │ Hi: {} │ Lv: {} │ {}",
        bold(color(32, "SPACE INVADERS")),
        bold(color(33, s.score.to_string())),
        bold(color(33, s.high_score.to_string())),
        bold(color(36, s.level.to_string())),
        color(31, "♥".repeat(lives))
    );
    let border = |left: &str, right: &str| dim(format!(" {left}{}{right}", "─".repeat(WIDTH as usize)));
    let mut lines = vec![border("╭", "╮"), box_line(&header), border("├", "┤")];
    for y in 0..HEIGHT {
        let row: String = (0..WIDTH).map(|x| cell(s, x, y)).collect();
        lines.push(format!("{}{row}{}", dim(" │"), dim("│")));
    }
    lines.push(border("├", "┤"));
    let footer = if inner.paused {
        format!(
            "{} Press any key to continue, {} to quit",
            color(33, bold("PAUSED")),
            bold("Q")
        )
    } else if s.game_over {
        format!(
            "{} Press {} to restart, {} to quit",
            color(31, bold("GAME OVER!")),
            bold("R"),
            bold("Q")
        )
    } else if s.victory {
        format!(
            "{} Press {} for level {}, {} to quit",
            color(32, bold("VICTORY!")),
            bold("R"),
            s.level + 1,
            bold("Q")
        )
    } else {
        format!(
            "←→ or AD to move, {}/F to fire, {} pause, {} quit",
            bold("SPACE"),
            bold("ESC"),
            bold("Q")
        )
    };
    lines.push(box_line(&footer));
    lines.push(border("╰", "╯"));
    lines
        .into_iter()
        .map(|line| {
            let len = strip_terminal_sequences(&line).encode_utf16().count();
            format!("{line}{}", " ".repeat(width.saturating_sub(len)))
        })
        .collect()
}
