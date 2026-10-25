//! Opt-in native tic-tac-toe. Register its factory explicitly; never loaded by default.
use pi_agent_core::{AgentToolResult, harness::types::ToolExecutionMode};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor as C};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{
    Text,
    keys::matches_key,
    tui::{Component, ComponentRef, component},
    utils::{truncate_to_width, visible_width},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

const SAVE_TYPE: &str = "tic-tac-toe-save";
const MOVE_TYPE: &str = "tic-tac-toe-move";
const OVER_TYPE: &str = "tic-tac-toe-game-over";
type Board = [[char; 3]; 3];
const WIN_LINES: [[(usize, usize); 3]; 8] = [
    [(0, 0), (0, 1), (0, 2)],
    [(1, 0), (1, 1), (1, 2)],
    [(2, 0), (2, 1), (2, 2)],
    [(0, 0), (1, 0), (2, 0)],
    [(0, 1), (1, 1), (2, 1)],
    [(0, 2), (1, 2), (2, 2)],
    [(0, 0), (1, 1), (2, 2)],
    [(0, 2), (1, 1), (2, 0)],
];
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Details {
    board: Board,
    agent_cursor_row: usize,
    agent_cursor_col: usize,
    status: String,
    current_turn: char,
}
#[derive(Clone)]
struct State {
    details: Details,
    user_row: usize,
    user_col: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            details: Details {
                board: [[' '; 3]; 3],
                agent_cursor_row: 0,
                agent_cursor_col: 0,
                status: "playing".into(),
                current_turn: 'X',
            },
            user_row: 1,
            user_col: 1,
        }
    }
}
struct Game {
    state: Arc<Mutex<State>>,
    active: bool,
    component: Option<Arc<Mutex<ComponentData>>>,
}
impl Default for Game {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            active: false,
            component: None,
        }
    }
}
type SharedGame = Arc<Mutex<Game>>;
struct ComponentData {
    state: Arc<Mutex<State>>,
    tui: ExtensionUiTui,
    version: u64,
    cached_version: Option<u64>,
    cached_width: usize,
    cached_lines: Vec<JsString>,
}
struct GameComponent {
    data: Arc<Mutex<ComponentData>>,
    game: SharedGame,
    api: ExtensionApi,
    done: ExtensionUiDone,
}
fn current_state(game: &SharedGame) -> Arc<Mutex<State>> {
    game.lock().unwrap().state.clone()
}
fn details(game: &SharedGame) -> Details {
    current_state(game).lock().unwrap().details.clone()
}
fn update_component(game: &SharedGame) {
    let (state, component) = {
        let game = game.lock().unwrap();
        (game.state.clone(), game.component.clone())
    };
    if let Some(component) = component {
        let mut component = component.lock().unwrap();
        component.state = state;
        component.version += 1;
        component.tui.request_render();
    }
}
fn persist(api: &ExtensionApi, game: &SharedGame) -> Result<(), HandlerError> {
    api.append_entry(&JsString::from(SAVE_TYPE), Some(JsValue::from(json!(details(game)))))
}
fn win_line(board: &Board) -> Option<[(usize, usize); 3]> {
    WIN_LINES.into_iter().find(|line| {
        let mark = board[line[0].0][line[0].1];
        mark != ' ' && line.iter().all(|&(r, c)| board[r][c] == mark)
    })
}
fn check_win(board: &Board) -> String {
    if let Some(line) = win_line(board) {
        return if board[line[0].0][line[0].1] == 'X' {
            "win_X"
        } else {
            "win_O"
        }
        .into();
    }
    if board.iter().flatten().all(|&mark| mark != ' ') {
        "draw"
    } else {
        "playing"
    }
    .into()
}
fn board_ascii(d: &Details) -> String {
    d.board
        .iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(|(c, &mark)| {
                    let cursor = r == d.agent_cursor_row && c == d.agent_cursor_col;
                    match (mark, cursor) {
                        (' ', true) => format!("<[{r},{c}]>"),
                        (' ', false) => format!(" [{r},{c}] "),
                        (_, true) => format!("   <{mark}>   "),
                        (_, false) => format!("    {mark}    "),
                    }
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>()
        .join("\n---------+---------+---------\n")
}
fn restore(game: &SharedGame, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let entries = ctx.session_manager()?.get_branch(None);
    let mut state = State::default();
    for entry in entries {
        let entry = serde_json::to_value(entry).map_err(|e| HandlerError::new(e.to_string()))?;
        let msg = &entry["message"];
        if msg["role"] != "toolResult"
            || !matches!(msg["toolName"].as_str(), Some("tic_tac_toe" | "tic_tac_toe_see_board"))
            || msg["details"].is_null()
        {
            continue;
        }
        state.details = serde_json::from_value(msg["details"].clone()).map_err(|e| HandlerError::new(e.to_string()))?;
    }
    let mut game = game.lock().unwrap();
    game.state = Arc::new(Mutex::new(state));
    game.active = false;
    Ok(())
}
fn send_over(api: &ExtensionApi, game: &SharedGame) -> Result<(), HandlerError> {
    let d = details(game);
    let label = match d.status.as_str() {
        "win_X" => "Player X (human) wins",
        "win_O" => "Player O (agent) wins",
        "draw" => "Draw",
        _ => "Game over",
    };
    api.send_message(
        CustomMessagePayload {
            custom_type: OVER_TYPE.into(),
            content: CustomMessageContent::Text(JsString::from(format!("Game over: {label}."))),
            display: true,
            details: Some(JsValue::from(json!(d))),
        },
        None,
    )
}
fn user_play(api: &ExtensionApi, game: &SharedGame, row: usize, col: usize) -> Result<(), HandlerError> {
    {
        let state = current_state(game);
        let mut state = state.lock().unwrap();
        let d = &mut state.details;
        d.board[row][col] = 'X';
        d.status = check_win(&d.board);
        if d.status == "playing" {
            d.current_turn = 'O';
        }
    }
    update_component(game);
    persist(api, game)?;
    let d = details(game);
    if d.status != "playing" {
        send_over(api, game)?;
        game.lock().unwrap().active = false;
        return Ok(());
    }
    let board = board_ascii(&d);
    let (r, c) = (d.agent_cursor_row, d.agent_cursor_col);
    api.send_message(CustomMessagePayload {custom_type:MOVE_TYPE.into(),display:true,details:Some(JsValue::from(json!(d))),content:CustomMessageContent::Text(JsString::from(format!(
        "Player X played at (row={row}, col={col}). It is now Player O's turn.\n\nBoard (your cursor marked with <>):\n{board}\n\nYour cursor is at (row={r}, col={c}). Decide your target cell, then emit every move_* and the final play as separate tic_tac_toe tool calls in THIS response."
    )))},Some(SendMessageOptions {trigger_turn:Some(true),..Default::default()}))
}
impl GameComponent {
    fn close(&self) {
        let mut game = self.game.lock().unwrap();
        game.component = None;
        game.active = false;
        drop(game);
        self.done.call(None);
    }
}
impl ExtensionUiComponent for GameComponent {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut data = self.data.lock().unwrap();
        if data.cached_width == width && data.cached_version == Some(data.version) {
            return data
                .cached_lines
                .clone()
                .into_iter()
                .map(JsString::from)
                .collect::<Vec<_>>();
        }
        let state = data.state.lock().unwrap().clone();
        let lines = render_game(&state, width);
        data.cached_width = width;
        data.cached_version = Some(data.version);
        data.cached_lines = lines.clone();
        lines.into_iter().map(JsString::from).collect::<Vec<_>>()
    }
    fn handle_input(&self, key: &pi_js_compat::js_json::JsString) {
        if matches_key(key, "escape") || (key == "q" || key == "Q") {
            self.close();
            return;
        }
        let mut data = self.data.lock().unwrap();
        let state = data.state.clone();
        let mut state = state.lock().unwrap();
        if state.details.status != "playing" {
            drop(state);
            drop(data);
            if key == "r" || key == "R" {
                self.close();
            }
            return;
        }
        if state.details.current_turn != 'X' {
            return;
        }
        let mut moved = true;
        if matches_key(key, "up") && state.user_row > 0 {
            state.user_row -= 1;
        } else if matches_key(key, "down") && state.user_row < 2 {
            state.user_row += 1;
        } else if matches_key(key, "left") && state.user_col > 0 {
            state.user_col -= 1;
        } else if matches_key(key, "right") && state.user_col < 2 {
            state.user_col += 1;
        } else {
            moved = false;
        }
        let play =
            (matches_key(key, "return") || key == " ") && state.details.board[state.user_row][state.user_col] == ' ';
        let (row, col) = (state.user_row, state.user_col);
        if moved {
            data.version += 1;
            data.tui.request_render();
        }
        drop(state);
        drop(data);
        if play {
            user_play(&self.api, &self.game, row, col).unwrap_or_else(|e| panic!("tic-tac-toe input: {e}"));
        }
    }
    fn invalidate(&self) {
        self.data.lock().unwrap().cached_width = 0;
    }
}
fn text_result(text: String, d: Details) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(JsValue::from(json!(d))),
        is_error: None,
        terminate: None,
    }
}
fn move_cursor(d: &mut Details, action: &JsString) -> String {
    let direction = match action {
        action if action == "move_up" => {
            d.agent_cursor_row = d.agent_cursor_row.saturating_sub(1);
            "up"
        }
        action if action == "move_down" => {
            d.agent_cursor_row = (d.agent_cursor_row + 1).min(2);
            "down"
        }
        action if action == "move_left" => {
            d.agent_cursor_col = d.agent_cursor_col.saturating_sub(1);
            "left"
        }
        action if action == "move_right" => {
            d.agent_cursor_col = (d.agent_cursor_col + 1).min(2);
            "right"
        }
        _ => unreachable!("validated action"),
    };
    format!(
        "Moved {direction}. Cursor: ({}, {})",
        d.agent_cursor_row, d.agent_cursor_col
    )
}
fn agent_play(d: &mut Details) -> Result<String, HandlerError> {
    if d.status != "playing" {
        return Err(HandlerError::new(format!("Game is over ({}).", d.status)));
    }
    if d.current_turn != 'O' {
        return Err(HandlerError::new("It is not your turn."));
    }
    let (r, c) = (d.agent_cursor_row, d.agent_cursor_col);
    if d.board[r][c] != ' ' {
        return Err(HandlerError::new(format!(
            "Cell ({r},{c}) is already {}. Your cursor is still at ({r},{c}). Move to an empty cell and retry play.",
            d.board[r][c]
        )));
    }
    d.board[r][c] = 'O';
    d.status = check_win(&d.board);
    d.agent_cursor_row = 0;
    d.agent_cursor_col = 0;
    let suffix = match d.status.as_str() {
        "playing" => {
            d.current_turn = 'X';
            "Cursor reset to (0,0). Your turn, X!"
        }
        "win_O" => "Player O wins!",
        "draw" => "It's a draw!",
        _ => "",
    };
    Ok(if suffix.is_empty() {
        format!("Placed O at ({r},{c}).")
    } else {
        format!("Placed O at ({r},{c}). {suffix}")
    })
}
async fn execute_action(
    api: &ExtensionApi,
    game: &SharedGame,
    action: &JsString,
) -> Result<AgentToolResult, HandlerError> {
    if action != "play" {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    let (result, occupied) = {
        let state = current_state(game);
        let mut state = state.lock().unwrap();
        let d = &mut state.details;
        let occupied = action == "play"
            && d.status == "playing"
            && d.current_turn == 'O'
            && d.board[d.agent_cursor_row][d.agent_cursor_col] != ' ';
        let result = if action == "play" {
            agent_play(d)
        } else {
            Ok(move_cursor(d, action))
        };
        (result, occupied)
    };
    if let Err(error) = result {
        if occupied {
            update_component(game);
            persist(api, game)?;
        }
        return Err(error);
    }
    if action == "play" && matches!(details(game).status.as_str(), "win_O" | "draw") {
        game.lock().unwrap().active = false;
        send_over(api, game)?;
    }
    update_component(game);
    persist(api, game)?;
    Ok(text_result(result.unwrap(), details(game)))
}
fn see_board(game: &SharedGame) -> AgentToolResult {
    let d = details(game);
    let board = board_ascii(&d);
    let turn = if d.current_turn == 'O' {
        "Player O (you)"
    } else {
        "Player X"
    };
    let text = format!(
        "Board (your cursor marked with <>):\n{board}\n\nYour cursor: (row={}, col={})\nStatus: {}\nTurn: {turn}",
        d.agent_cursor_row, d.agent_cursor_col, d.status
    );
    text_result(text, d)
}
fn text_component(text: JsString) -> ComponentRef {
    component(Text::new(text, 0, 0))
}
fn first_text(result: &AgentToolResult) -> JsString {
    match result.content.first() {
        Some(TextOrImageContent::Text(t)) => t.text.clone(),
        _ => JsString::default(),
    }
}
fn result_details(result: &AgentToolResult) -> Option<Details> {
    result
        .details
        .as_ref()
        .filter(|v| !v.is_null())
        .map(|v| pi_js_compat::js_json::from_value(v).expect("persisted board details"))
}

pub fn tic_tac_toe_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let game = Arc::new(Mutex::new(Game::default()));
    let state = game.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            Box::pin(async move {
                restore(&state, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let state = game.clone();
    pi.on_session_tree(
        move |_: SessionTreeEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            Box::pin(async move {
                restore(&state, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let state = game.clone();
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              _: ExtensionContext|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let state = state.clone();
            Box::pin(async move {
                if !state.lock().unwrap().active {
                    return Ok(None);
                }
                Ok(Some(BeforeAgentStartEventResult {
                    system_prompt: Some(event.system_prompt + INSTRUCTIONS),
                    ..Default::default()
                }))
            })
        },
    )?;
    pi.register_message_renderer(
        MOVE_TYPE,
        Arc::new(|message, options, theme| {
            let d = message
                .details
                .as_ref()
                .filter(|v| !v.is_null())
                .map(|v| pi_js_compat::js_json::from_value::<Details>(v).expect("persisted board details"));
            let turn = if d.as_ref().is_some_and(|d| d.current_turn == 'O') {
                js_format!("{} (Agent)", theme.fg(C::Warning, &theme.bold("O")))
            } else {
                js_format!("{} (You)", theme.fg(C::Accent, &theme.bold("X")))
            };
            let title = js_format!(
                "{}  {}  next: {}",
                theme.fg(C::Accent, &theme.bold("Player X played")),
                theme.fg(C::Dim, "→"),
                turn
            );
            Some(component(Banner {
                title,
                details: d,
                expanded: options.expanded,
                theme: theme.clone(),
            }))
        }),
    )?;
    pi.register_message_renderer(
        OVER_TYPE,
        Arc::new(|message, _, theme| {
            let d = message
                .details
                .as_ref()
                .filter(|v| !v.is_null())
                .map(|v| pi_js_compat::js_json::from_value::<Details>(v).expect("persisted board details"));
            Some(component(GameOver {
                status: d.as_ref().map_or("draw", |d| d.status.as_str()).into(),
                details: d,
                theme: theme.clone(),
            }))
        }),
    )?;
    let state = game.clone();
    let api = pi.clone();
    pi.register_command(
        "tic-tac-toe",
        Some("Play tic-tac-toe against the agent".into()),
        None,
        Arc::new(move |_, ctx| {
            let game = state.clone();
            let api = api.clone();
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?.notify(
                        &JsString::from("Tic-tac-toe requires interactive mode"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(());
                }
                restore(&game, &ctx)?;
                if details(&game).status != "playing" {
                    *current_state(&game).lock().unwrap() = State::default();
                }
                game.lock().unwrap().active = true;
                api.set_session_name(&JsString::from("Tic-Tac-Toe"))?;
                ctx.ui()?
                    .custom(
                        Arc::new(move |context, done| {
                            let data = Arc::new(Mutex::new(ComponentData {
                                state: current_state(&game),
                                tui: context.tui,
                                version: 0,
                                cached_version: None,
                                cached_width: 0,
                                cached_lines: vec![],
                            }));
                            game.lock().unwrap().component = Some(data.clone());
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(GameComponent {
                                data,
                                game: game.clone(),
                                api: api.clone(),
                                done,
                            }))
                        }),
                        None,
                    )
                    .await?;
                Ok(())
            })
        }),
    )?;
    let state = game.clone();
    let api = pi.clone();
    pi.register_tool(ToolDefinition {
        name:"tic_tac_toe".into(),label:"Tic-Tac-Toe".into(),description:ACTION_DESCRIPTION.into(),
        prompt_snippet:Some("Play a tic-tac-toe action (move_up/down/left/right or play) as Player O".into()),
        prompt_guidelines:Some(vec!["When it is your tic-tac-toe turn, decide the target cell first, then emit every move_* plus the final play as separate tic_tac_toe tool calls in a SINGLE assistant response. Never split them across responses or wait for intermediate results.".into(),"Never ask the user for the board. The board and your cursor position are included in the user's move message; use tic_tac_toe_see_board if you need them restated.".into()]),
        parameters:json!({"type":"object","properties":{"action":{"type":"string","enum":["move_up","move_down","move_left","move_right","play"],"description":"The single action to perform this call. Emit multiple tic_tac_toe calls in one response to string actions together."}},"required":["action"]}),
        constrained_sampling:None,approval_required:None,approval_kind:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:Some(ToolExecutionMode::Sequential),
        execute:Arc::new(move|_,params,_,_,_| {let game=state.clone();let api=api.clone();Box::pin(async move{execute_action(&api,&game,params["action"].as_string().expect("validated action")).await})}),
        render_call:Some(infallible_render_call(|args,theme,_|text_component(theme.fg(C::ToolTitle,&theme.bold("tic_tac_toe "))+&theme.fg(C::Muted,args["action"].as_string().unwrap_or(&JsString::from("")))))),
        render_result:Some(infallible_render_result(|result,options,theme,context| {
            let prefix=if context.is_error {theme.fg(C::Error,"✗ ")} else {theme.fg(C::Success,"✓ ")};
            let summary=prefix+&theme.fg(C::Muted,first_text(result));
            render_tool_result(summary,result,options.expanded,theme)
        })),
    })?;
    pi.register_tool(ToolDefinition {
        name:"tic_tac_toe_see_board".into(),label:"See Board".into(),description:"Return the current tic-tac-toe board state and YOUR cursor position (Player O). Takes no arguments. Use this if you need the current state restated mid-turn (for example after a failed play). The user's cursor is never exposed.".into(),
        prompt_snippet:Some("Inspect the tic-tac-toe board and your cursor".into()),prompt_guidelines:None,
        parameters:json!({"type":"object","properties":{}}),constrained_sampling:None,approval_required:None,approval_kind:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
        execute:Arc::new(move|_,_,_,_,_| {let game=game.clone();Box::pin(async move{Ok(see_board(&game))})}),
        render_call:Some(infallible_render_call(|_,theme,_|text_component(theme.fg(C::ToolTitle,&theme.bold("tic_tac_toe_see_board"))))),
        render_result:Some(infallible_render_result(|result,options,theme,_| {
            let d=result_details(result);
            let (r,c)=d.as_ref().map_or((0,0),|d|(d.agent_cursor_row,d.agent_cursor_col));
            let summary=theme.fg(C::Success,"✓ ")+&theme.fg(C::Muted,&format!("cursor ({r},{c})"));
            render_tool_result(summary,result,options.expanded,theme)
        })),
    })
}
fn render_tool_result(summary: JsString, result: &AgentToolResult, expanded: bool, theme: &Theme) -> ComponentRef {
    if let Some(details) = result_details(result).filter(|_| expanded) {
        return component(Banner {
            title: summary,
            details: Some(details),
            expanded: true,
            theme: theme.clone(),
        });
    }
    text_component(summary)
}

fn sgr(code: &str, text: &str) -> String {
    format!("\x1b[{code}m{text}\x1b[0m")
}
fn dim(text: &str) -> String {
    format!("\x1b[2m{text}\x1b[22m")
}
fn center(text: impl Into<JsString>, width: usize) -> JsString {
    let text = text.into();
    let len = visible_width(&text);
    if len >= width {
        return truncate_to_width(&text, width);
    }
    let padding = width - len;
    js_format!(
        "{}{}{}",
        " ".repeat(padding / 2),
        text,
        " ".repeat(padding - padding / 2)
    )
}
fn cell_color(board: &Board, win: &Option<[(usize, usize); 3]>, r: usize, c: usize) -> Option<&'static str> {
    if board[r][c] == ' ' {
        None
    } else if win.as_ref().is_some_and(|line| line.contains(&(r, c))) {
        Some("32")
    } else if board[r][c] == 'X' {
        Some("34")
    } else {
        Some("33")
    }
}
fn border_color(board: &Board, win: &Option<[(usize, usize); 3]>, cells: &[(usize, usize)]) -> Option<&'static str> {
    let colors = cells
        .iter()
        .filter_map(|&(r, c)| cell_color(board, win, r, c))
        .collect::<Vec<_>>();
    colors
        .first()
        .copied()
        .filter(|first| colors.iter().all(|color| color == first))
}
fn paint_border(text: &str, cursor: bool, agent: bool, color: Option<&str>) -> String {
    if cursor {
        sgr(if agent { "33;1" } else { "32;1" }, text)
    } else if let Some(color) = color {
        sgr(&format!("{color};1"), text)
    } else {
        dim(text)
    }
}
fn corner(r: usize, c: usize) -> &'static str {
    match (r, c) {
        (0, 0) => "┌",
        (0, 3) => "┐",
        (3, 0) => "└",
        (3, 3) => "┘",
        (0, _) => "┬",
        (3, _) => "┴",
        (_, 0) => "├",
        (_, 3) => "┤",
        _ => "┼",
    }
}
fn adjacent(r: usize, c: usize) -> Vec<(usize, usize)> {
    [(-1, -1), (-1, 0), (0, -1), (0, 0)]
        .into_iter()
        .filter_map(|(dr, dc)| {
            let (r, c) = (r as isize + dr, c as isize + dc);
            (r >= 0 && c >= 0 && r < 3 && c < 3).then_some((r as usize, c as usize))
        })
        .collect()
}
fn cell_content(board: &Board, win: &Option<[(usize, usize); 3]>, r: usize, c: usize, line: usize) -> String {
    let mark = board[r][c];
    if mark == ' ' || line != 1 {
        return " ".repeat(7);
    }
    let glyph = if mark == 'X' { "╳" } else { "◯" };
    let len = 7 - visible_width(glyph);
    let left = len / 2;
    format!(
        "{}{}{}",
        " ".repeat(left),
        sgr(&format!("{};1", cell_color(board, win, r, c).unwrap()), glyph),
        " ".repeat(len - left)
    )
}
fn render_board(board: &Board, width: usize, cursor: Option<(usize, usize, bool)>) -> Vec<JsString> {
    let win = win_line(board);
    let mut lines = vec![];
    for gr in 0..=3 {
        let mut row = String::new();
        for gc in 0..=3 {
            let highlight = cursor.is_some_and(|(r, c, _)| (gr == r || gr == r + 1) && (gc == c || gc == c + 1));
            let agent = cursor.is_some_and(|(_, _, agent)| agent);
            row += &paint_border(
                corner(gr, gc),
                highlight,
                agent,
                border_color(board, &win, &adjacent(gr, gc)),
            );
            if gc == 3 {
                continue;
            }
            let cells = [gr.checked_sub(1).map(|r| (r, gc)), (gr < 3).then_some((gr, gc))]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let highlight = cursor.is_some_and(|(r, c, _)| gc == c && (gr == r || gr == r + 1));
            row += &paint_border(&"─".repeat(7), highlight, agent, border_color(board, &win, &cells));
        }
        lines.push(center(&row, width));
        if gr == 3 {
            break;
        }
        for line in 0..3 {
            let mut row = String::new();
            for gc in 0usize..=3 {
                let cells = [gc.checked_sub(1).map(|c| (gr, c)), (gc < 3).then_some((gr, gc))]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                let highlight = cursor.is_some_and(|(r, c, _)| gr == r && (gc == c || gc == c + 1));
                row += &paint_border(
                    "│",
                    highlight,
                    cursor.is_some_and(|(_, _, agent)| agent),
                    border_color(board, &win, &cells),
                );
                if gc < 3 {
                    row += &cell_content(board, &win, gr, gc, line);
                }
            }
            lines.push(center(&row, width));
        }
    }
    lines
}
fn render_game(state: &State, width: usize) -> Vec<JsString> {
    let title = " Tic-Tac-Toe ";
    let fill = width.saturating_sub(visible_width(title));
    let d = &state.details;
    let status = if d.status != "playing" {
        match d.status.as_str() {
            "draw" => sgr("1", &sgr("33", "Draw!")),
            "win_X" => sgr("1", &sgr("32", "X wins!")),
            _ => sgr("1", &sgr("33", "O wins!")),
        }
    } else if d.current_turn == 'X' {
        format!(
            "Turn: {} (You)  {}  {} (Agent)",
            sgr("1", &sgr("34", "X")),
            sgr("2", "|"),
            sgr("1", &sgr("33", "O"))
        )
    } else {
        format!(
            "{} (You)  {}  Turn: {} (Agent)",
            sgr("34", "X"),
            sgr("2", "|"),
            sgr("1", &sgr("33", "O"))
        )
    };
    let mut lines = vec![
        JsString::from(
            sgr("2", &"─".repeat(fill / 2)) + &sgr("1", &sgr("34", title)) + &sgr("2", &"─".repeat(fill - fill / 2)),
        ),
        "".into(),
        center(&status, width),
        "".into(),
        "".into(),
    ];
    let cursor = if d.status != "playing" {
        None
    } else if d.current_turn == 'X' {
        Some((state.user_row, state.user_col, false))
    } else {
        Some((d.agent_cursor_row, d.agent_cursor_col, true))
    };
    lines.extend(render_board(&d.board, width, cursor));
    lines.extend(["".into(), "".into()]);
    let footer = if d.status != "playing" {
        format!(
            "{} restart  {}  {}/{} quit",
            sgr("1", "R"),
            sgr("2", "|"),
            sgr("1", "Q"),
            sgr("1", "ESC")
        )
    } else if d.current_turn != 'X' {
        sgr("2", "Agent is thinking...")
    } else {
        format!(
            "{} move  {}  {} play  {}  {} quit",
            sgr("1", "←↑↓→"),
            sgr("2", "|"),
            sgr("1", "ENTER"),
            sgr("2", "|"),
            sgr("1", "ESC")
        )
    };
    lines.extend([
        center(&footer, width),
        "".into(),
        JsString::from(sgr("2", &"─".repeat(width))),
    ]);
    lines
}
struct Banner {
    title: JsString,
    details: Option<Details>,
    expanded: bool,
    theme: Theme,
}
impl Component for Banner {
    fn invalidate(&mut self) {}
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let fill = width.saturating_sub(visible_width(&self.title) + 2);
        let mut lines = vec![js_format!(
            "{} {} {}",
            self.theme.fg(C::Dim, &"─".repeat(fill / 2)),
            self.title,
            self.theme.fg(C::Dim, &"─".repeat(fill - fill / 2))
        )];
        if let Some(d) = self.details.as_ref().filter(|_| self.expanded) {
            lines.push("".into());
            lines.extend(render_board(&d.board, width, None));
        }
        lines
    }
}
struct GameOver {
    status: String,
    details: Option<Details>,
    theme: Theme,
}
impl Component for GameOver {
    fn invalidate(&mut self) {}
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let hr = self.theme.fg(C::Dim, &"─".repeat(width));
        let (title, sub) = match self.status.as_str() {
            "win_X" => (
                self.theme.bold(&self.theme.fg(C::Accent, "★ Player X wins ★")),
                "You beat the agent.",
            ),
            "win_O" => (
                self.theme.bold(&self.theme.fg(C::Warning, "★ Player O wins ★")),
                "The agent beat you.",
            ),
            "draw" => (self.theme.bold(&self.theme.fg(C::Muted, "— Draw —")), "No winner."),
            _ => (self.theme.bold("Game over"), ""),
        };
        let mut lines = vec![hr.clone(), "".into()];
        for text in [title, self.theme.fg(C::Dim, sub)] {
            lines.push(JsString::from(
                JsString::from(" ".repeat(width.saturating_sub(visible_width(&text)) / 2)) + &text,
            ));
        }
        lines.push("".into());
        if let Some(d) = &self.details {
            lines.extend(render_board(&d.board, width, None));
            lines.push("".into());
        }
        lines.push(hr);
        lines
    }
}
const ACTION_DESCRIPTION: &str = "Execute ONE tic-tac-toe action as Player O. `action` is exactly one of: move_up, move_down, move_left, move_right (move YOUR cursor one cell, clamped at edges), or play (place O under YOUR cursor; errors if the cell is not empty). There is no batched form. To play at (r, c) from your current cursor (r0, c0), emit the required move_down/move_up and move_right/move_left calls, then play, all as separate tool_use blocks in the SAME assistant response. Do not split the sequence across responses and do not wait for a result before emitting the next call. Your cursor position persists between turns and is reset to (0,0) only after a successful play.";
const INSTRUCTIONS: &str = include_str!("tic_tac_toe_instructions.txt");
