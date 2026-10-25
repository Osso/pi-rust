//! Opt-in Snake example. No discovery or default loading.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{keys::matches_key, utils::visible_width};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
const WIDTH: i32 = 40;
const HEIGHT: i32 = 15;
const TICK: Duration = Duration::from_millis(100);
type Random = Arc<dyn Fn() -> f64 + Send + Sync>;
type Save = Arc<dyn Fn(Option<State>) + Send + Sync>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Point {
    x: i32,
    y: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct State {
    snake: Vec<Point>,
    food: Point,
    direction: Direction,
    next_direction: Direction,
    score: i64,
    game_over: bool,
    high_score: i64,
}
fn random_unit() -> f64 {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).expect("OS randomness for Snake");
    (u64::from_ne_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}
pub fn snake_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    snake_extension_with_random(pi, Arc::new(random_unit))
}
pub(crate) fn snake_extension_with_random(pi: &ExtensionApi, random: Random) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.register_command(
        "snake",
        Some("Play Snake!".into()),
        None,
        Arc::new(move |_, ctx| {
            let api = api.clone();
            let random = random.clone();
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?.notify(
                        &JsString::from("Snake requires interactive mode"),
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
                    .find(|e| e["type"] == "custom" && e["customType"] == "snake-save")
                    .map(|e| e["data"].clone())
                    .filter(|v| !v.is_null());
                let saved: Option<State> = saved
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| HandlerError::new(e.to_string()))?;
                // appendEntry is infallible in the TS callback. Surface native write errors
                // through the command rather than discarding them in the component callback.
                let error = Arc::new(Mutex::new(None));
                let captured = error.clone();
                let save: Save = Arc::new(move |state| {
                    if let Err(e) = api.append_entry(
                        &JsString::from("snake-save"),
                        Some(JsValue::from(state.map_or(Value::Null, |s| json!(s)))),
                    ) {
                        *captured.lock().unwrap() = Some(e);
                    }
                });
                ctx.ui()?
                    .custom(
                        Arc::new(move |context, done| {
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(Snake::new(
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
fn spawn_food(snake: &[Point], random: &Random) -> Point {
    loop {
        let food = Point {
            x: (random() * WIDTH as f64).floor() as i32,
            y: (random() * HEIGHT as f64).floor() as i32,
        };
        if !snake.contains(&food) {
            return food;
        }
    }
}
fn initial(random: &Random) -> State {
    let head = Point {
        x: WIDTH / 2,
        y: HEIGHT / 2,
    };
    State {
        snake: vec![
            head,
            Point {
                x: head.x - 1,
                y: head.y,
            },
            Point {
                x: head.x - 2,
                y: head.y,
            },
        ],
        food: spawn_food(&[head], random),
        direction: Direction::Right,
        next_direction: Direction::Right,
        score: 0,
        game_over: false,
        high_score: 0,
    }
}
fn tick(s: &mut State, random: &Random) {
    s.direction = s.next_direction;
    let mut head = s.snake[0];
    match s.direction {
        Direction::Up => head.y -= 1,
        Direction::Down => head.y += 1,
        Direction::Left => head.x -= 1,
        Direction::Right => head.x += 1,
    }
    if head.x < 0 || head.x >= WIDTH || head.y < 0 || head.y >= HEIGHT || s.snake.contains(&head) {
        s.game_over = true;
        return;
    }
    s.snake.insert(0, head);
    if head == s.food {
        s.score += 10;
        s.high_score = s.high_score.max(s.score);
        s.food = spawn_food(&s.snake, random)
    } else {
        s.snake.pop();
    }
}
struct Inner {
    state: State,
    paused: bool,
    version: u64,
    cached_version: Option<u64>,
    cached_width: usize,
    cached_lines: Vec<String>,
}
struct Snake {
    inner: Arc<Mutex<Inner>>,
    timer: Mutex<Option<JoinHandle<()>>>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    save: Save,
    random: Random,
}
impl Snake {
    fn new(tui: ExtensionUiTui, done: ExtensionUiDone, save: Save, saved: Option<State>, random: Random) -> Self {
        let paused = saved.as_ref().is_some_and(|s| !s.game_over);
        let state = if paused {
            saved.unwrap()
        } else {
            let mut s = initial(&random);
            s.high_score = saved.map_or(0, |s| s.high_score);
            s
        };
        let component = Self {
            inner: Arc::new(Mutex::new(Inner {
                state,
                paused,
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
            component.start_game()
        }
        component
    }
    fn start_game(&self) {
        let inner = self.inner.clone();
        let tui = self.tui.clone();
        let random = self.random.clone();
        let start = tokio::time::Instant::now() + TICK;
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval_at(start, TICK);
            loop {
                interval.tick().await;
                let mut inner = inner.lock().unwrap();
                if !inner.state.game_over {
                    tick(&mut inner.state, &random);
                    inner.version += 1;
                    tui.request_render();
                }
            }
        });
        *self.timer.lock().unwrap() = Some(task);
    }
    fn stop(&self) {
        if let Some(task) = self.timer.lock().unwrap().take() {
            task.abort();
        }
    }
}
impl Drop for Snake {
    fn drop(&mut self) {
        self.stop();
    }
}
impl ExtensionUiComponent for Snake {
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let mut inner = self.inner.lock().unwrap();
        if inner.paused {
            if matches_key(data, "escape") || (data == "q" || data == "Q") {
                self.stop();
                self.done.call(None);
                return;
            }
            inner.paused = false;
            drop(inner);
            self.start_game();
            return;
        }
        if matches_key(data, "escape") {
            self.stop();
            (self.save)(Some(inner.state.clone()));
            self.done.call(None);
            return;
        }
        if data == "q" || data == "Q" {
            self.stop();
            (self.save)(None);
            self.done.call(None);
            return;
        }
        let s = &mut inner.state;
        let direction = if matches_key(data, "up") || (data == "w" || data == "W") {
            Some(Direction::Up)
        } else if matches_key(data, "down") || (data == "s" || data == "S") {
            Some(Direction::Down)
        } else if matches_key(data, "right") || (data == "d" || data == "D") {
            Some(Direction::Right)
        } else if matches_key(data, "left") || (data == "a" || data == "A") {
            Some(Direction::Left)
        } else {
            None
        };
        if let Some(next) = direction {
            let opposite = matches!(
                (s.direction, next),
                (Direction::Up, Direction::Down)
                    | (Direction::Down, Direction::Up)
                    | (Direction::Left, Direction::Right)
                    | (Direction::Right, Direction::Left)
            );
            if !opposite {
                s.next_direction = next;
            }
        }
        if s.game_over && (data == "r" || data == "R" || data == " ") {
            let high = s.high_score;
            *s = initial(&self.random);
            s.high_score = high;
            (self.save)(None);
            inner.version += 1;
            self.tui.request_render();
        }
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        if width < 4 {
            let count = (width as i32 - 4).div_euclid(2) * 2;
            pi_coding_agent::utils::uncaught::throw_uncaught("RangeError", format!("Invalid count value: {count}"));
        }
        let mut inner = self.inner.lock().unwrap();
        if width == inner.cached_width && inner.cached_version == Some(inner.version) {
            return inner
                .cached_lines
                .clone()
                .into_iter()
                .map(JsString::from)
                .collect::<Vec<_>>();
        }
        let lines = render_state(&inner.state, inner.paused, width);
        inner.cached_lines = lines.clone();
        inner.cached_width = width;
        inner.cached_version = Some(inner.version);
        lines.into_iter().map(JsString::from).collect::<Vec<_>>()
    }
    fn invalidate(&self) {
        self.inner.lock().unwrap().cached_width = 0;
    }
    fn dispose(&self) {
        self.stop();
    }
}
fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[22m")
}
fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}
fn red(s: &str) -> String {
    format!("\x1b[31m{s}\x1b[0m")
}
fn yellow(s: &str) -> String {
    format!("\x1b[33m{s}\x1b[0m")
}
fn bold(s: &str) -> String {
    format!("\x1b[1m{s}\x1b[22m")
}
fn pad(line: String, width: usize) -> String {
    // Source padLine counts UTF-16 code units after removing SGR, not wcwidth.
    let plain = pi_coding_agent::utils::ansi::strip_ansi(&JsString::from(&line));
    let len = plain.encode_utf16().count();
    line + &" ".repeat(width.saturating_sub(len))
}
fn box_line(content: &str, box_width: usize) -> String {
    dim(" │") + content + &" ".repeat(box_width.saturating_sub(visible_width(content))) + &dim("│")
}
fn render_state(s: &State, paused: bool, width: usize) -> Vec<String> {
    let effective = WIDTH.min((width as i32 - 4).div_euclid(2)) as usize;
    let box_width = effective * 2;
    let horizontal = "─".repeat(box_width);
    let mut lines = vec![pad(dim(&format!(" ╭{horizontal}╮")), width)];
    let title = format!(
        "{} │ Score: {} │ High: {}",
        bold(&green("SNAKE")),
        bold(&yellow(&s.score.to_string())),
        bold(&yellow(&s.high_score.to_string()))
    );
    lines.push(pad(box_line(&title, box_width), width));
    lines.push(pad(dim(&format!(" ├{horizontal}┤")), width));
    for y in 0..HEIGHT {
        let mut row = String::new();
        for x in 0..effective {
            let point = Point { x: x as i32, y };
            row += &if s.snake[0] == point {
                green("██")
            } else if s.snake[1..].contains(&point) {
                green("▓▓")
            } else if s.food == point {
                red("◆ ")
            } else {
                "  ".into()
            };
        }
        lines.push(pad(dim(" │") + &row + &dim("│"), width));
    }
    lines.push(pad(dim(&format!(" ├{horizontal}┤")), width));
    let footer = if paused {
        format!(
            "{} Press any key to continue, {} to quit",
            yellow(&bold("PAUSED")),
            bold("Q")
        )
    } else if s.game_over {
        format!(
            "{} Press {} to restart, {} to quit",
            red(&bold("GAME OVER!")),
            bold("R"),
            bold("Q")
        )
    } else {
        format!("↑↓←→ or WASD to move, {} pause, {} quit", bold("ESC"), bold("Q"))
    };
    lines.push(pad(box_line(&footer, box_width), width));
    lines.push(pad(dim(&format!(" ╰{horizontal}╯")), width));
    lines
}
