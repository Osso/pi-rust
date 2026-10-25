//! Differential test: the real TypeScript `Editor` vs the Rust port.
//!
//! Each scenario drives scripted input, renders, pastes, resizes, history, and
//! autocomplete through both implementations and compares exact rendered lines
//! (including ANSI), text, cursor, callbacks, and autocomplete visibility.

use crate::shared_editor_host as editor_host;

use crate::shared_oracle as oracle_support;

use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::time::Duration;

use editor_host::fake_host;
use oracle_support::TsOracle;
use pi_tui::autocomplete::{
    AutocompleteFuture, AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
    CompletionResult,
};
use pi_tui::components::editor::{Editor, EditorOptions, EditorTheme};
use pi_tui::components::loader::LoaderIndicatorOptions;
use pi_tui::tui::{Component, Focusable};
use serde_json::{Value, json};

struct ScenarioProvider {
    items: Vec<AutocompleteItem>,
    mode: String,
    require_force: bool,
    should_trigger: Option<bool>,
    trigger_characters: Vec<JsString>,
    prefix: Option<JsString>,
}

impl AutocompleteProvider for ScenarioProvider {
    fn trigger_characters(&self) -> Vec<JsString> {
        self.trigger_characters.clone()
    }

    fn should_trigger_file_completion(
        &self,
        _lines: &[JsString],
        _cursor_line: usize,
        _cursor_col: usize,
    ) -> Option<bool> {
        self.should_trigger
    }

    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<JsString>,
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture {
        Box::pin(async move {
            if self.require_force && !options.force {
                return None;
            }
            let line = lines.get(cursor_line).cloned().unwrap_or_default();
            let prefix = line.slice(0, cursor_col);
            let match_prefix = self.prefix.as_ref().unwrap_or(&prefix);
            let items: Vec<AutocompleteItem> = if self.mode == "prefix" {
                self.items
                    .iter()
                    .filter(|item| item.value.starts_with(match_prefix))
                    .cloned()
                    .collect()
            } else {
                self.items.clone()
            };
            if items.is_empty() {
                return None;
            }
            Some(AutocompleteSuggestions {
                items,
                prefix: self.prefix.clone().unwrap_or_else(|| prefix.clone()),
            })
        })
    }

    fn apply_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult {
        let line = lines.get(cursor_line).cloned().unwrap_or_default();
        let before = line.slice(0, cursor_col - prefix.len());
        let after = line.slice(cursor_col, line.len());
        let mut new_lines = lines.to_vec();
        new_lines[cursor_line] = before + &item.value + after;
        CompletionResult {
            lines: new_lines,
            cursor_line,
            cursor_col: cursor_col - prefix.len() + item.value.len(),
        }
    }
}

fn scenario_editor(options: &Value, styling: &Value) -> Editor {
    let (host, _) = fake_host(24);
    let mut theme = EditorTheme::plain();
    if styling["promptPrefix"].as_bool().unwrap_or(false) {
        theme.prompt_prefix = Some(Rc::new(|text| text.clone()));
    }
    if styling["borderColor"].as_bool().unwrap_or(false) {
        theme.border_color = Rc::new(|text| JsString::from("\x1b[35m") + text + "\x1b[39m");
    }
    Editor::new(
        host,
        theme,
        EditorOptions {
            padding_x: options
                .get("paddingX")
                .and_then(Value::as_u64)
                .map(|value| value as usize),
            autocomplete_max_visible: options
                .get("autocompleteMaxVisible")
                .and_then(Value::as_u64)
                .map(|value| value as usize),
        },
    )
}

fn provider_from_request(request: &Value) -> Rc<dyn AutocompleteProvider> {
    let items = request["items"]
        .as_array()
        .expect("provider items")
        .iter()
        .map(|item| {
            let mut result = AutocompleteItem::new(
                item["value"].as_str().unwrap_or_default(),
                item["label"].as_str().unwrap_or_default(),
            );
            if let Some(description) = item.get("description").and_then(Value::as_str) {
                result.description = Some(description.into());
            }
            result
        })
        .collect();
    Rc::new(ScenarioProvider {
        items,
        mode: request["mode"].as_str().unwrap_or("all").to_string(),
        require_force: request.get("requireForce").and_then(Value::as_bool).unwrap_or(false),
        should_trigger: request.get("shouldTrigger").and_then(Value::as_bool),
        trigger_characters: request
            .get("triggerCharacters")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).map(JsString::from).collect())
            .unwrap_or_default(),
        prefix: request.get("prefix").and_then(Value::as_str).map(JsString::from),
    })
}

async fn run_local(request: &Value) -> Value {
    let options = request.get("options").cloned().unwrap_or(json!({}));
    let mut editor = scenario_editor(&options, &request["theme"]);
    let changes = Rc::new(std::cell::RefCell::new(Vec::<JsString>::new()));
    let submissions = Rc::new(std::cell::RefCell::new(Vec::<JsString>::new()));
    let change_log = Rc::clone(&changes);
    editor.on_change = Some(Rc::new(move |text| {
        change_log.borrow_mut().push(text.clone());
    }));
    let submit_log = Rc::clone(&submissions);
    editor.on_submit = Some(Rc::new(move |text| {
        submit_log.borrow_mut().push(text.clone());
    }));

    let mut renders: Vec<Vec<JsString>> = Vec::new();
    for operation in request["ops"].as_array().expect("ops array") {
        match operation["op"].as_str().expect("op name") {
            "input" => editor.handle_input(&pi_js_compat::js_json::JsString::from(
                operation["data"].as_str().unwrap_or_default(),
            )),
            "setText" => editor.set_text(operation["text"].as_str().unwrap_or_default()),
            "insertText" => editor.insert_text_at_cursor(operation["text"].as_str().unwrap_or_default()),
            "addHistory" => editor.add_to_history(operation["text"].as_str().unwrap_or_default()),
            "setHistory" => {
                let entries = operation["entries"]
                    .as_array()
                    .expect("history entries")
                    .iter()
                    .map(|entry| JsString::from(entry.as_str().unwrap_or_default()))
                    .collect();
                editor.set_history(entries);
            }
            "render" => {
                let width = operation["width"].as_u64().expect("render width") as usize;
                renders.push(editor.render(width));
            }
            "settle" => {
                tokio::time::sleep(Duration::from_millis(60)).await;
                editor.poll_autocomplete().await;
            }
            "focus" => editor.set_focused(operation["focused"].as_bool().unwrap_or(false)),
            "setProvider" => editor.set_autocomplete_provider(provider_from_request(&operation["provider"])),
            "setPaddingX" => editor.set_padding_x(operation["padding"].as_u64().unwrap_or(0) as usize),
            "setAutocompleteMaxVisible" => {
                editor.set_autocomplete_max_visible(operation["maxVisible"].as_u64().unwrap_or(5) as usize)
            }
            "setWorking" => editor.set_working(operation["working"].as_bool().unwrap_or(false)),
            "setWorkingIndicator" => {
                let frames = operation["frames"].as_array().map(|frames| {
                    frames
                        .iter()
                        .map(|frame| JsString::from(frame.as_str().unwrap_or_default()))
                        .collect()
                });
                editor
                    .set_working_indicator(Some(LoaderIndicatorOptions {
                        frames,
                        interval_ms: operation.get("intervalMs").and_then(Value::as_u64),
                    }))
                    .expect("valid working frames");
            }
            "setScreenOrigin" => editor.set_screen_origin(
                operation["row"].as_i64().unwrap_or(0),
                operation["col"].as_i64().unwrap_or(0),
            ),
            "clearScreenOrigin" => editor.clear_screen_origin(),
            other => panic!("unknown editor operation {other}"),
        }
    }

    let (line, col) = editor.get_cursor();
    json!({
        "renders": renders,
        "text": editor.get_text(),
        "expanded": editor.get_expanded_text(),
        "cursor": { "line": line, "col": col },
        "changes": changes.borrow().clone(),
        "submissions": submissions.borrow().clone(),
        "autocomplete": editor.is_showing_autocomplete(),
    })
}

fn input(data: &str) -> Value {
    json!({"op": "input", "data": data})
}

fn render(width: usize) -> Value {
    json!({"op": "render", "width": width})
}

fn scenario(ops: Vec<Value>) -> Value {
    json!({ "ops": ops })
}

fn scenarios() -> Vec<(&'static str, Value)> {
    let mut cases = Vec::new();

    let mut scroll_ops = vec![
        json!({"op":"setText", "text": (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")}),
        render(10),
    ];
    scroll_ops.extend((0..10).map(|_| input("\x1b[A")));
    scroll_ops.push(render(10));
    cases.push(("frozen-narrow-top-and-bottom-indicators", scenario(scroll_ops)));

    cases.push((
        "typing",
        scenario(vec![
            input("h"),
            input("e"),
            input("l"),
            input("l"),
            input("o"),
            render(40),
            input("\x1b[D"),
            input("X"),
            render(40),
        ]),
    ));
    cases.push((
        "unicode-wrapping",
        scenario(vec![
            json!({"op":"setText","text":"Hello ✅ 日本語 äöü 😀"}),
            render(20),
            render(10),
            input("\x1b[D"),
            input("\x1b[D"),
            input("x"),
            render(12),
        ]),
    ));
    cases.push((
        "word-wrap",
        scenario(vec![
            json!({"op":"setText","text":"Lorem ipsum dolor sit amet, consectetur adipiscing elit sed do eiusmod"}),
            render(30),
            render(15),
            render(8),
        ]),
    ));
    cases.push((
        "prompt-prefix-padding",
        json!({
            "theme": {"promptPrefix": true},
            "options": {"paddingX": 2},
            "ops": [
                json!({"op":"setText","text":"hello world"}),
                render(20),
                render(11),
                json!({"op":"setScreenOrigin","row":3,"col":4}),
                json!({"op":"setWorking","working":true}),
                render(20)
            ]
        }),
    ));
    cases.push((
        "focus-cursor-marker",
        scenario(vec![
            json!({"op":"setText","text":"abc"}),
            json!({"op":"focus","focused":true}),
            render(10),
            json!({"op":"focus","focused":false}),
            render(10),
        ]),
    ));
    cases.push((
        "scroll-indicators",
        json!({
            "ops": [
                json!({"op":"setText","text": (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")}),
                render(10),
                input("\x1b[A"), input("\x1b[A"), input("\x1b[A"), input("\x1b[A"),
                input("\x1b[A"), input("\x1b[A"), input("\x1b[A"), input("\x1b[A"),
                input("\x1b[A"), input("\x1b[A"),
                render(10),
                render(3)
            ]
        }),
    ));
    cases.push((
        "history",
        scenario(vec![
            json!({"op":"addHistory","text":"older entry"}),
            json!({"op":"addHistory","text":"line1\nline2\nline3"}),
            json!({"op":"setText","text":"draft"}),
            input("\x1b[A"),
            input("\x1b[A"),
            input("\x1b[A"),
            render(40),
            input("\x1b[B"),
            render(40),
            input("\x1b[B"),
            render(40),
        ]),
    ));
    cases.push((
        "history-search",
        scenario(vec![
            json!({"op":"addHistory","text":"older config note\nhidden detail"}),
            json!({"op":"addHistory","text":"newer config fix"}),
            json!({"op":"addHistory","text":"check tests"}),
            input("\x12"),
            input("c"),
            input("o"),
            render(40),
            input("\x12"),
            render(40),
            input("\r"),
            render(40),
        ]),
    ));
    cases.push((
        "kill-ring",
        scenario(vec![
            json!({"op":"setText","text":"one two three"}),
            input("\x17"),
            input("\x17"),
            input("\x17"),
            input("\x19"),
            input("\x1by"),
            render(40),
        ]),
    ));
    cases.push((
        "undo",
        scenario(vec![
            input("h"),
            input("e"),
            input("l"),
            input("l"),
            input("o"),
            input(" "),
            input("w"),
            input("o"),
            input("r"),
            input("l"),
            input("d"),
            input("\x1b[45;5u"),
            render(40),
            input("\x1b[45;5u"),
            render(40),
        ]),
    ));
    cases.push((
        "paste-marker",
        json!({
            "ops": [
                input("A"),
                json!({"op":"input","data": format!("\x1b[200~{}\x1b[201~", "line\n".repeat(20).trim_end())}),
                input("B"),
                render(40),
                input("\x01"),
                input("\x1b[C"),
                input("\x1b[C"),
                input("\x7f"),
                render(40),
                input("\x1b[45;5u"),
                render(40)
            ]
        }),
    ));
    cases.push((
        "paste-submit",
        json!({
            "ops": [
                json!({"op":"input","data": format!("\x1b[200~{}\x1b[201~", "line\n".repeat(12).trim_end())}),
                input("\r"),
                render(20)
            ]
        }),
    ));
    cases.push((
        "jump",
        scenario(vec![
            json!({"op":"setText","text":"hello world\nfoo bar"}),
            input("\x01"),
            input("\x1d"),
            input("o"),
            render(40),
            input("\x1b\x1d"),
            input("a"),
            render(40),
        ]),
    ));
    cases.push((
        "sticky-column",
        scenario(vec![
            json!({"op":"setText","text":"1234567890\n\n1234567890"}),
            input("\x01"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[A"),
            input("\x1b[A"),
            render(40),
            input("\x1b[B"),
            input("\x1b[B"),
            render(40),
        ]),
    ));
    cases.push((
        "set-text-normalization",
        scenario(vec![
            json!({"op":"setText","text":"a\r\nb\rc\td"}),
            render(20),
            input("\x1b[45;5u"),
            render(20),
        ]),
    ));
    cases.push((
        "insert-text",
        scenario(vec![
            json!({"op":"setText","text":"hello world"}),
            input("\x01"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            input("\x1b[C"),
            json!({"op":"insertText","text":"line1\nline2\nline3"}),
            render(30),
            input("\x1b[45;5u"),
            render(30),
        ]),
    ));
    cases.push((
        "autocomplete-slash-menu",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"/help","label":"help","description":"Show help"},{"value":"/model","label":"model","description":"Switch model"}],"mode":"prefix"}}),
                input("/"),
                json!({"op":"settle"}),
                render(60),
                input("\t"),
                render(60)
            ]
        }),
    ));
    cases.push((
        "autocomplete-force-single",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"Workspace/","label":"Workspace/"}],"mode":"prefix","requireForce":true}}),
                input("W"), input("o"), input("r"), input("k"),
                json!({"op":"settle"}),
                render(40),
                input("\t"),
                json!({"op":"settle"}),
                render(40),
                input("\x1b[45;5u"),
                render(40)
            ]
        }),
    ));
    cases.push((
        "autocomplete-force-multiple",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"src/","label":"src/"},{"value":"src.txt","label":"src.txt"}],"mode":"prefix","requireForce":true}}),
                input("s"), input("r"), input("c"),
                input("\t"),
                json!({"op":"settle"}),
                render(50),
                input("\t"),
                render(50)
            ]
        }),
    ));
    cases.push((
        "autocomplete-debounce-at",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"@main.ts","label":"main.ts"}],"mode":"all"}}),
                input("@"), input("m"), input("a"), input("i"),
                render(40),
                json!({"op":"settle"}),
                render(40)
            ]
        }),
    ));
    cases.push((
        "autocomplete-custom-trigger",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"$skill-name","label":"skill-name"}],"mode":"all","triggerCharacters":["$"]}}),
                input("$"), input("s"), input("k"),
                render(40),
                json!({"op":"settle"}),
                render(40)
            ]
        }),
    ));
    cases.push((
        "autocomplete-should-not-trigger",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"src/","label":"src/"}],"mode":"all","requireForce":true,"shouldTrigger":false}}),
                input("s"), input("r"), input("c"),
                input("\t"),
                json!({"op":"settle"}),
                render(40)
            ]
        }),
    ));
    cases.push((
        "autocomplete-requery-on-left",
        json!({
            "ops": [
                json!({"op":"setProvider","provider":{"items":[{"value":"cmd","label":"cmd"},{"value":"repo","label":"repo"},{"value":"message","label":"message"}],"mode":"all"}}),
                input("/"), input("c"), input("m"), input("d"), input(" "),
                json!({"op":"settle"}),
                render(60),
                input("\x1b[D"),
                json!({"op":"settle"}),
                render(60)
            ]
        }),
    ));
    cases.push((
        "working-static",
        json!({
            "theme": {"promptPrefix": true},
            "ops": [
                json!({"op":"setWorkingIndicator","frames":["●"]}),
                json!({"op":"setWorking","working":true}),
                render(20),
                json!({"op":"setWorking","working":false}),
                render(20)
            ]
        }),
    ));
    cases.push((
        "cjk-word-movement",
        scenario(vec![
            json!({"op":"setText","text":"hello你好，world世界"}),
            input("\x1b[1;5D"),
            input("\x1b[1;5D"),
            input("\x1b[1;5C"),
            render(40),
        ]),
    ));
    cases.push((
        "emoji-editing",
        scenario(vec![
            input("😀"),
            input("👍"),
            input("🎉"),
            input("\x1b[D"),
            input("\x1b[D"),
            input("x"),
            input("\x7f"),
            render(20),
        ]),
    ));
    cases.push((
        "page-and-line-movement",
        scenario(vec![
            json!({"op":"setText","text":"1\n2\n3\n4\n5\n6\n7\n8\n9\n10"}),
            input("\x01"),
            input("\x1b[6~"),
            render(20),
            input("\x1b[5~"),
            input("\x05"),
            render(20),
        ]),
    ));
    cases.push((
        "backslash-enter",
        scenario(vec![input("\\"), input("\r"), input("x"), input("\r"), render(20)]),
    ));

    cases
}

#[tokio::test]
async fn approved_bottom_indicator_fits_narrow_colored_frames_like_typescript() {
    let mut oracle = TsOracle::spawn("editor-oracle.mts", "packages/tui/src");
    for (line_count, up_steps) in [(20, 10), (120, 40)] {
        for width in [3, 7, 10, 12, 13, 20] {
            let mut ops = vec![
                json!({"op":"setText","text":(0..line_count).map(|index|format!("line {index}")).collect::<Vec<_>>().join("\n")}),
                render(width),
            ];
            ops.extend((0..up_steps).map(|_| input("\x1b[A")));
            ops.push(render(width));
            let request = json!({"id":width,"ops":ops,"theme":{"borderColor":true}});
            let response = oracle.request(&request);
            assert_eq!(response["ok"], true, "{response}");
            let expected = &response["output"];
            for line in expected["renders"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_array()
                .unwrap()
            {
                assert_eq!(
                    pi_tui::utils::visible_width(JsString::from(line.as_str().unwrap())),
                    width,
                    "TS {line_count}/{width}: {line}"
                );
            }
            let actual = run_local(&request).await;
            assert_eq!(actual, *expected, "delta {line_count}/{width}");
        }
    }
}

#[tokio::test]
async fn editor_matches_typescript_oracle() {
    let mut oracle = TsOracle::spawn("editor-oracle.mts", "packages/tui/src");
    for (index, (name, request)) in scenarios().into_iter().enumerate() {
        let payload = json!({ "id": index, "ops": request["ops"], "theme": request.get("theme"), "options": request.get("options") });
        let response = oracle.request(&payload);
        assert_eq!(response["ok"], json!(true), "{name}: oracle failed: {response}");
        let expected = response["output"].clone();
        let actual = run_local(&request).await;
        assert_eq!(actual, expected, "scenario {name} diverged");
    }
}
