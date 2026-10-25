//! Ported from `coding-agent/src/modes/interactive/components/armin.ts`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::tui::Component;

use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::random::{random_f64, random_index};

// XBM image: 31x36 pixels, LSB first, 1=background, 0=foreground.
const WIDTH: usize = 31;
const HEIGHT: usize = 36;
const BITS: [u8; 144] = [
    0xff, 0xff, 0xff, 0x7f, 0xff, 0xf0, 0xff, 0x7f, 0xff, 0xed, 0xff, 0x7f, 0xff, 0xdb, 0xff, 0x7f, 0xff, 0xb7, 0xff,
    0x7f, 0xff, 0x77, 0xfe, 0x7f, 0x3f, 0xf8, 0xfe, 0x7f, 0xdf, 0xff, 0xfe, 0x7f, 0xdf, 0x3f, 0xfc, 0x7f, 0x9f, 0xc3,
    0xfb, 0x7f, 0x6f, 0xfc, 0xf4, 0x7f, 0xf7, 0x0f, 0xf7, 0x7f, 0xf7, 0xff, 0xf7, 0x7f, 0xf7, 0xff, 0xe3, 0x7f, 0xf7,
    0x07, 0xe8, 0x7f, 0xef, 0xf8, 0x67, 0x70, 0x0f, 0xff, 0xbb, 0x6f, 0xf1, 0x00, 0xd0, 0x5b, 0xfd, 0x3f, 0xec, 0x53,
    0xc1, 0xff, 0xef, 0x57, 0x9f, 0xfd, 0xee, 0x5f, 0x9f, 0xfc, 0xae, 0x5f, 0x1f, 0x78, 0xac, 0x5f, 0x3f, 0x00, 0x50,
    0x6c, 0x7f, 0x00, 0xdc, 0x77, 0xff, 0xc0, 0x3f, 0x78, 0xff, 0x01, 0xf8, 0x7f, 0xff, 0x03, 0x9c, 0x78, 0xff, 0x07,
    0x8c, 0x7c, 0xff, 0x0f, 0xce, 0x78, 0xff, 0xff, 0xcf, 0x7f, 0xff, 0xff, 0xcf, 0x78, 0xff, 0xff, 0xdf, 0x78, 0xff,
    0xff, 0xdf, 0x7d, 0xff, 0xff, 0x3f, 0x7e, 0xff, 0xff, 0xff, 0x7f,
];

const BYTES_PER_ROW: usize = WIDTH.div_ceil(8);
const DISPLAY_HEIGHT: usize = HEIGHT.div_ceil(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Typewriter,
    Scanline,
    Rain,
    Fade,
    Crt,
    Glitch,
    Dissolve,
}

const EFFECTS: [Effect; 7] = [
    Effect::Typewriter,
    Effect::Scanline,
    Effect::Rain,
    Effect::Fade,
    Effect::Crt,
    Effect::Glitch,
    Effect::Dissolve,
];

#[derive(Clone, Copy)]
struct Drop {
    y: isize,
    settled: usize,
}

enum EffectState {
    Typewriter { pos: usize },
    Scanline { row: usize },
    Rain { drops: Vec<Drop> },
    Fade { positions: Vec<(usize, usize)>, idx: usize },
    Crt { expansion: usize },
    Glitch { phase: usize, glitch_frames: usize },
    Dissolve { positions: Vec<(usize, usize)>, idx: usize },
}

fn get_pixel(x: usize, y: usize) -> bool {
    if y >= HEIGHT {
        return false;
    }
    let byte_index = y * BYTES_PER_ROW + x / 8;
    let bit_index = x % 8;
    ((BITS[byte_index] >> bit_index) & 1) == 0
}

fn get_char(x: usize, row: usize) -> &'static str {
    let upper = get_pixel(x, row * 2);
    let lower = get_pixel(x, row * 2 + 1);
    match (upper, lower) {
        (true, true) => "\u{2588}",
        (true, false) => "\u{2580}",
        (false, true) => "\u{2584}",
        (false, false) => " ",
    }
}

fn build_final_grid() -> Vec<Vec<JsString>> {
    (0..DISPLAY_HEIGHT)
        .map(|row| (0..WIDTH).map(|x| get_char(x, row).into()).collect())
        .collect()
}

fn empty_grid() -> Vec<Vec<JsString>> {
    vec![vec![JsString::from(" "); WIDTH]; DISPLAY_HEIGHT]
}

fn shuffled_positions() -> Vec<(usize, usize)> {
    let mut positions: Vec<(usize, usize)> = (0..DISPLAY_HEIGHT)
        .flat_map(|row| (0..WIDTH).map(move |x| (row, x)))
        .collect();
    for index in (1..positions.len()).rev() {
        let swap = random_index(index + 1);
        positions.swap(index, swap);
    }
    positions
}

/// "Armin says hi" animated XBM easter egg.
pub struct ArminComponent {
    request_render: Rc<dyn Fn()>,
    effect: Effect,
    final_grid: Vec<Vec<JsString>>,
    current_grid: Vec<Vec<JsString>>,
    effect_state: EffectState,
    cached_lines: Vec<JsString>,
    cached_width: Option<usize>,
    grid_version: usize,
    cached_version: Option<usize>,
    interval: Duration,
    next_tick: Option<Instant>,
}

impl ArminComponent {
    pub fn new(request_render: Rc<dyn Fn()>) -> Self {
        let effect = EFFECTS[random_index(EFFECTS.len())];
        let final_grid = build_final_grid();
        let current_grid = empty_grid();
        let interval = Duration::from_secs_f64(1.0 / if effect == Effect::Glitch { 60.0 } else { 30.0 });
        let mut component = Self {
            request_render,
            effect,
            final_grid,
            current_grid,
            effect_state: EffectState::Scanline { row: 0 },
            cached_lines: Vec::new(),
            cached_width: None,
            grid_version: 0,
            cached_version: None,
            interval,
            next_tick: Some(Instant::now() + interval),
        };
        component.init_effect();
        component
    }

    pub fn effect(&self) -> &'static str {
        match self.effect {
            Effect::Typewriter => "typewriter",
            Effect::Scanline => "scanline",
            Effect::Rain => "rain",
            Effect::Fade => "fade",
            Effect::Crt => "crt",
            Effect::Glitch => "glitch",
            Effect::Dissolve => "dissolve",
        }
    }

    pub fn is_running(&self) -> bool {
        self.next_tick.is_some()
    }

    pub fn current_grid(&self) -> &[Vec<JsString>] {
        &self.current_grid
    }

    /// Advance the animation when the frame deadline passed.
    pub fn poll(&mut self, now: Instant) -> bool {
        let Some(mut next_tick) = self.next_tick else {
            return false;
        };
        let mut ticked = false;
        while now >= next_tick {
            let done = self.tick_effect();
            self.update_display();
            (self.request_render)();
            ticked = true;
            if done {
                self.stop_animation();
                break;
            }
            next_tick += self.interval;
        }
        if self.next_tick.is_some() {
            self.next_tick = Some(next_tick);
        }
        ticked
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.next_tick
    }

    fn stop_animation(&mut self) {
        self.next_tick = None;
    }

    pub fn dispose(&mut self) {
        self.stop_animation();
    }

    fn init_effect(&mut self) {
        self.effect_state = match self.effect {
            Effect::Typewriter => EffectState::Typewriter { pos: 0 },
            Effect::Scanline => EffectState::Scanline { row: 0 },
            Effect::Rain => EffectState::Rain {
                drops: (0..WIDTH)
                    .map(|_| Drop {
                        y: -((random_f64() * (DISPLAY_HEIGHT * 2) as f64).floor() as isize),
                        settled: 0,
                    })
                    .collect(),
            },
            Effect::Fade => EffectState::Fade {
                positions: shuffled_positions(),
                idx: 0,
            },
            Effect::Crt => EffectState::Crt { expansion: 0 },
            Effect::Glitch => EffectState::Glitch {
                phase: 0,
                glitch_frames: 8,
            },
            Effect::Dissolve => {
                let characters = [
                    " ", "\u{2591}", "\u{2592}", "\u{2593}", "\u{2588}", "\u{2580}", "\u{2584}",
                ];
                self.current_grid = (0..DISPLAY_HEIGHT)
                    .map(|_| {
                        (0..WIDTH)
                            .map(|_| characters[random_index(characters.len())].into())
                            .collect()
                    })
                    .collect();
                EffectState::Dissolve {
                    positions: shuffled_positions(),
                    idx: 0,
                }
            }
        };
    }

    fn tick_effect(&mut self) -> bool {
        match &mut self.effect_state {
            EffectState::Typewriter { pos } => {
                let mut position = *pos;
                for _ in 0..3 {
                    let row = position / WIDTH;
                    let x = position % WIDTH;
                    if row >= DISPLAY_HEIGHT {
                        return true;
                    }
                    self.current_grid[row][x] = self.final_grid[row][x].clone();
                    position += 1;
                }
                *pos = position;
                false
            }
            EffectState::Scanline { row } => {
                if *row >= DISPLAY_HEIGHT {
                    return true;
                }
                self.current_grid[*row] = self.final_grid[*row].clone();
                *row += 1;
                false
            }
            EffectState::Rain { drops } => {
                let mut all_settled = true;
                self.current_grid = empty_grid();
                for (x, drop) in drops.iter_mut().enumerate() {
                    let drop = &mut *drop;
                    let mut row = DISPLAY_HEIGHT as isize - 1;
                    while row >= (DISPLAY_HEIGHT as isize - drop.settled as isize) {
                        if row >= 0 {
                            self.current_grid[row as usize][x] = self.final_grid[row as usize][x].clone();
                        }
                        row -= 1;
                    }
                    if drop.settled >= DISPLAY_HEIGHT {
                        continue;
                    }
                    all_settled = false;

                    let mut target_row: isize = -1;
                    let mut candidate = DISPLAY_HEIGHT as isize - 1 - drop.settled as isize;
                    while candidate >= 0 {
                        if self.final_grid[candidate as usize][x] != " " {
                            target_row = candidate;
                            break;
                        }
                        candidate -= 1;
                    }

                    drop.y += 1;
                    if drop.y >= 0 && (drop.y as usize) < DISPLAY_HEIGHT {
                        if target_row >= 0 && drop.y >= target_row {
                            drop.settled = DISPLAY_HEIGHT - target_row as usize;
                            drop.y = -((random_f64() * 5.0).floor() as isize) - 1;
                        } else {
                            self.current_grid[drop.y as usize][x] = "\u{2593}".into();
                        }
                    }
                }
                all_settled
            }
            EffectState::Fade { positions, idx } => {
                for _ in 0..15 {
                    if *idx >= positions.len() {
                        return true;
                    }
                    let (row, x) = positions[*idx];
                    self.current_grid[row][x] = self.final_grid[row][x].clone();
                    *idx += 1;
                }
                false
            }
            EffectState::Crt { expansion } => {
                let mid_row = DISPLAY_HEIGHT / 2;
                self.current_grid = empty_grid();
                let top = mid_row as isize - *expansion as isize;
                let bottom = mid_row + *expansion;
                let start = top.max(0) as usize;
                let end = bottom.min(DISPLAY_HEIGHT - 1);
                for row in start..=end {
                    self.current_grid[row] = self.final_grid[row].clone();
                }
                *expansion += 1;
                *expansion > DISPLAY_HEIGHT
            }
            EffectState::Glitch { phase, glitch_frames } => {
                if *phase < *glitch_frames {
                    self.current_grid = self
                        .final_grid
                        .iter()
                        .map(|row| {
                            let offset = (random_f64() * 7.0).floor() as isize - 3;
                            if random_f64() < 0.3 {
                                let mut shifted = Vec::with_capacity(row.len());
                                if offset >= 0 {
                                    shifted.extend_from_slice(&row[offset as usize..]);
                                    shifted.extend_from_slice(&row[..offset as usize]);
                                } else {
                                    let split = (row.len() as isize + offset) as usize;
                                    shifted.extend_from_slice(&row[split..]);
                                    shifted.extend_from_slice(&row[..split]);
                                }
                                shifted.truncate(WIDTH);
                                return shifted;
                            }
                            if random_f64() < 0.2 {
                                let swap_row = random_index(DISPLAY_HEIGHT);
                                return self.final_grid[swap_row].clone();
                            }
                            row.clone()
                        })
                        .collect();
                    *phase += 1;
                    return false;
                }
                self.current_grid = self.final_grid.clone();
                true
            }
            EffectState::Dissolve { positions, idx } => {
                for _ in 0..20 {
                    if *idx >= positions.len() {
                        return true;
                    }
                    let (row, x) = positions[*idx];
                    self.current_grid[row][x] = self.final_grid[row][x].clone();
                    *idx += 1;
                }
                false
            }
        }
    }

    fn update_display(&mut self) {
        self.grid_version += 1;
    }

    fn clip_row(row: &[JsString], available_width: isize) -> JsString {
        let length = row.len() as isize;
        let end = if available_width < 0 {
            (length + available_width).max(0)
        } else {
            available_width.min(length)
        };
        JsString::join(&row[..end as usize], "")
    }
}

impl Component for ArminComponent {
    fn dispose(&mut self) {
        ArminComponent::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        if self.cached_width == Some(width) && self.cached_version == Some(self.grid_version) {
            return self.cached_lines.clone();
        }

        let padding = 1;
        let available_width = width as isize - padding as isize;
        let mut lines: Vec<JsString> = self
            .current_grid
            .iter()
            .map(|row| {
                let clipped = Self::clip_row(row, available_width);
                let pad_right = (width as isize - padding as isize - clipped.len() as isize).max(0) as usize;
                js_format!(
                    " {}{}",
                    with_theme(|theme| theme.fg(ThemeColor::Accent, &clipped)),
                    " ".repeat(pad_right)
                )
            })
            .collect();

        let message = "ARMIN SAYS HI";
        let message_pad_right = (width as isize - padding as isize - message.len() as isize).max(0) as usize;
        lines.push(js_format!(
            " {}{}",
            with_theme(|theme| theme.fg(ThemeColor::Accent, message)),
            " ".repeat(message_pad_right)
        ));

        self.cached_lines = lines.clone();
        self.cached_width = Some(width);
        self.cached_version = Some(self.grid_version);
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.cached_width = None;
    }
}
