//! Ported from `packages/tui/test/editor.test.ts` (`Autocomplete`, the
//! autocomplete paste case, and `undoes autocomplete`).

use crate::shared_editor_host as editor_host;

use pi_js_compat::js_json::JsString;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use editor_host::{editor, send};
use pi_tui::autocomplete::{
    AutocompleteFuture, AutocompleteItem, AutocompleteOptions, AutocompleteProvider, AutocompleteSuggestions,
    CombinedAutocompleteProvider, CommandEntry, CompletionResult, SlashCommand,
};
use pi_tui::components::editor::Editor;

fn apply_completion(
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

type SuggestionsFn = Rc<dyn Fn(Vec<JsString>, usize, usize, AutocompleteOptions) -> AutocompleteFuture>;
type ShouldTriggerFn = Rc<dyn Fn(&[JsString], usize, usize) -> bool>;

struct MockProvider {
    triggers: Vec<JsString>,
    should_trigger: Option<ShouldTriggerFn>,
    suggestions: SuggestionsFn,
}

impl AutocompleteProvider for MockProvider {
    fn trigger_characters(&self) -> Vec<JsString> {
        self.triggers.clone()
    }

    fn should_trigger_file_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
    ) -> Option<bool> {
        self.should_trigger
            .as_ref()
            .map(|should_trigger| should_trigger(lines, cursor_line, cursor_col))
    }

    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<JsString>,
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture {
        (self.suggestions)(lines, cursor_line, cursor_col, options)
    }

    fn apply_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult {
        apply_completion(lines, cursor_line, cursor_col, item, prefix)
    }
}

fn provider(suggestions: SuggestionsFn) -> Rc<dyn AutocompleteProvider> {
    Rc::new(MockProvider {
        triggers: Vec::new(),
        should_trigger: None,
        suggestions,
    })
}

fn immediate(suggestions: Option<AutocompleteSuggestions>) -> AutocompleteFuture {
    Box::pin(async move { suggestions })
}

async fn settle(editor: &mut Editor) {
    editor.poll_autocomplete().await;
}

fn rendered(editor: &mut Editor) -> JsString {
    use pi_tui::tui::Component;
    JsString::join(
        editor
            .render(80)
            .into_iter()
            .map(|line| pi_tui::utils::strip_terminal_sequences(&line)),
        "\n",
    )
}

#[tokio::test]
async fn auto_applies_single_force_file_suggestion_without_showing_menu() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, options| {
        if !options.force {
            return immediate(None);
        }
        let prefix = lines[0].slice(0, cursor_col);
        if prefix == "Work" {
            return immediate(Some(AutocompleteSuggestions {
                items: vec![AutocompleteItem::new("Workspace/", "Workspace/")],
                prefix: JsString::from("Work"),
            }));
        }
        immediate(None)
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["W", "o", "r", "k"] {
        send(&mut editor, ch);
    }
    assert_eq!(editor.get_text(), "Work");
    send(&mut editor, "\t");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "Workspace/");
    assert!(!editor.is_showing_autocomplete());
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "Work");
}

#[tokio::test]
async fn shows_menu_when_force_file_has_multiple_suggestions() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, options| {
        if !options.force {
            return immediate(None);
        }
        let prefix = lines[0].slice(0, cursor_col);
        if prefix == "src" {
            return immediate(Some(AutocompleteSuggestions {
                items: vec![
                    AutocompleteItem::new("src/", "src/"),
                    AutocompleteItem::new("src.txt", "src.txt"),
                ],
                prefix: JsString::from("src"),
            }));
        }
        immediate(None)
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["s", "r", "c"] {
        send(&mut editor, ch);
    }
    send(&mut editor, "\t");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "src");
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\t");
    assert_eq!(editor.get_text(), "src/");
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn keeps_suggestions_open_when_typing_in_force_mode() {
    let (mut editor, _) = editor();
    let all_files = [
        AutocompleteItem::new("readme.md", "readme.md"),
        AutocompleteItem::new("package.json", "package.json"),
        AutocompleteItem::new("src/", "src/"),
        AutocompleteItem::new("dist/", "dist/"),
    ];
    let mock = provider(Rc::new(move |lines, _line, cursor_col, options| {
        let prefix = lines[0].slice(0, cursor_col);
        let should_match = options.force || prefix.contains('/') || prefix.starts_with('.');
        if !should_match {
            return immediate(None);
        }
        let filtered: Vec<AutocompleteItem> = all_files
            .iter()
            .filter(|item| item.value.to_lowercase().starts_with(prefix.to_lowercase()))
            .cloned()
            .collect();
        if filtered.is_empty() {
            return immediate(None);
        }
        immediate(Some(AutocompleteSuggestions {
            items: filtered,
            prefix: prefix.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    send(&mut editor, "\t");
    settle(&mut editor).await;
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "r");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "r");
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "e");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "re");
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\t");
    assert_eq!(editor.get_text(), "readme.md");
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn debounces_at_autocomplete_while_typing() {
    let (mut editor, _) = editor();
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mock = provider(Rc::new(move |lines, _line, cursor_col, _options| {
        counter.set(counter.get() + 1);
        let prefix = lines[0].slice(0, cursor_col);
        immediate(Some(AutocompleteSuggestions {
            items: vec![AutocompleteItem::new("@main.ts", "main.ts")],
            prefix: prefix.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["@", "m", "a", "i"] {
        send(&mut editor, ch);
    }
    assert_eq!(calls.get(), 0);
    assert!(!editor.is_showing_autocomplete());
    tokio::time::sleep(Duration::from_millis(50)).await;
    settle(&mut editor).await;
    assert_eq!(calls.get(), 1);
    assert!(editor.is_showing_autocomplete());
}

#[tokio::test]
async fn requeries_the_autocomplete_picker_when_the_cursor_moves_back_into_the_command_name() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, _options| {
        let before = lines[0].slice(0, cursor_col);
        if !before.starts_with('/') {
            return immediate(None);
        }
        if before.contains(' ') {
            return immediate(Some(AutocompleteSuggestions {
                items: vec![
                    AutocompleteItem::new("repo", "repo"),
                    AutocompleteItem::new("message", "message"),
                    AutocompleteItem::new("help", "help"),
                ],
                prefix: before.slice(before.find(' ').unwrap() + 1, before.len()),
            }));
        }
        immediate(Some(AutocompleteSuggestions {
            items: vec![AutocompleteItem::new("cmd", "cmd")],
            prefix: before.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["/", "c", "m", "d", " "] {
        send(&mut editor, ch);
        settle(&mut editor).await;
    }
    assert_eq!(editor.get_text(), "/cmd ");
    assert!(editor.is_showing_autocomplete());
    let at_arg = rendered(&mut editor);
    assert!(at_arg.contains("repo"));
    send(&mut editor, "\x1b[D");
    settle(&mut editor).await;
    let after_move = rendered(&mut editor);
    assert!(!after_move.contains("repo"));
    assert!(!after_move.contains("message"));
}

#[tokio::test]
async fn debounces_hash_autocomplete_while_typing() {
    let (mut editor, _) = editor();
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mock = provider(Rc::new(move |lines, _line, cursor_col, _options| {
        counter.set(counter.get() + 1);
        let prefix = lines[0].slice(0, cursor_col);
        immediate(Some(AutocompleteSuggestions {
            items: vec![AutocompleteItem::new("#2983", "#2983")],
            prefix: prefix.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["#", "2", "9", "8"] {
        send(&mut editor, ch);
    }
    assert_eq!(calls.get(), 0);
    assert!(!editor.is_showing_autocomplete());
    tokio::time::sleep(Duration::from_millis(50)).await;
    settle(&mut editor).await;
    assert_eq!(calls.get(), 1);
    assert!(editor.is_showing_autocomplete());
}

#[tokio::test]
async fn debounces_custom_trigger_characters_autocomplete_while_typing() {
    let (mut editor, _) = editor();
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mock = Rc::new(MockProvider {
        triggers: vec![JsString::from("$")],
        should_trigger: None,
        suggestions: Rc::new(move |lines, _line, cursor_col, _options| {
            counter.set(counter.get() + 1);
            let prefix = lines[0].slice(0, cursor_col);
            immediate(Some(AutocompleteSuggestions {
                items: vec![AutocompleteItem::new("$skill-name", "skill-name")],
                prefix: prefix.clone(),
            }))
        }),
    });
    editor.set_autocomplete_provider(mock);
    send(&mut editor, "$");
    send(&mut editor, "s");
    send(&mut editor, "k");
    assert_eq!(calls.get(), 0);
    tokio::time::sleep(Duration::from_millis(50)).await;
    settle(&mut editor).await;
    assert_eq!(calls.get(), 1);
    assert!(editor.is_showing_autocomplete());
}

#[tokio::test]
async fn resets_custom_trigger_characters_when_provider_changes() {
    let (mut editor, _) = editor();
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    editor.set_autocomplete_provider(Rc::new(MockProvider {
        triggers: vec![JsString::from("$")],
        should_trigger: None,
        suggestions: Rc::new(|_, _, _, _| {
            immediate(Some(AutocompleteSuggestions {
                items: vec![AutocompleteItem::new("$skill-name", "skill-name")],
                prefix: JsString::from("$"),
            }))
        }),
    }));
    editor.set_autocomplete_provider(provider(Rc::new(move |_, _, _, _| {
        counter.set(counter.get() + 1);
        immediate(Some(AutocompleteSuggestions {
            items: vec![AutocompleteItem::new("$skill-name", "skill-name")],
            prefix: JsString::from("$"),
        }))
    })));
    send(&mut editor, "$");
    send(&mut editor, "s");
    tokio::time::sleep(Duration::from_millis(50)).await;
    settle(&mut editor).await;
    assert_eq!(calls.get(), 0);
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn aborts_active_at_autocomplete_when_typing_continues() {
    let (mut editor, _) = editor();
    let aborts = Rc::new(Cell::new(0));
    let counter = Rc::clone(&aborts);
    let mock = provider(Rc::new(move |_lines, _line, _cursor_col, options| {
        let signal = options.signal.clone();
        let counter = Rc::clone(&counter);
        Box::pin(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => Some(AutocompleteSuggestions {
                    items: vec![AutocompleteItem::new("@main.ts", "main.ts")],
                    prefix: JsString::from("@main"),
                }),
                _ = signal.wait() => {
                    counter.set(counter.get() + 1);
                    None
                }
            }
        })
    }));
    editor.set_autocomplete_provider(mock);
    for ch in ["@", "m", "a", "i"] {
        send(&mut editor, ch);
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    editor.begin_autocomplete_request();
    send(&mut editor, "n");
    tokio::time::sleep(Duration::from_millis(50)).await;
    settle(&mut editor).await;
    assert_eq!(aborts.get(), 1);
}

#[tokio::test]
async fn hides_autocomplete_when_backspacing_slash_command_to_empty() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, _options| {
        let prefix = lines[0].slice(0, cursor_col);
        if !prefix.starts_with('/') {
            return immediate(None);
        }
        let commands = [
            AutocompleteItem::new("/model", "model"),
            AutocompleteItem::new("/help", "help"),
        ];
        let query = prefix.slice(1, prefix.len());
        let filtered: Vec<AutocompleteItem> = commands
            .into_iter()
            .filter(|item| item.value.starts_with(&query))
            .collect();
        if filtered.is_empty() {
            return immediate(None);
        }
        immediate(Some(AutocompleteSuggestions {
            items: filtered,
            prefix: prefix.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    send(&mut editor, "/");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "/");
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\x7f");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "");
    assert!(!editor.is_showing_autocomplete());
}

async fn slash_argument_case(typed: &str, arguments: Vec<&str>, selected_exact: bool) -> JsString {
    let (mut editor, _) = editor();
    let arguments: Vec<JsString> = arguments.into_iter().map(JsString::from).collect();
    let mock = provider(Rc::new(move |lines, _line, cursor_col, _options| {
        let before = lines[0].slice(0, cursor_col);
        let Some(argument_text) = before.strip_prefix("/argtest ") else {
            return immediate(None);
        };
        let filtered: Vec<AutocompleteItem> = arguments
            .iter()
            .filter(|value| {
                if selected_exact {
                    **value == argument_text || value.starts_with(&argument_text)
                } else {
                    value.starts_with(&argument_text)
                }
            })
            .map(|value| AutocompleteItem::new(value.clone(), value.clone()))
            .collect();
        if filtered.is_empty() {
            return immediate(None);
        }
        immediate(Some(AutocompleteSuggestions {
            items: filtered,
            prefix: argument_text.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    for ch in typed.chars() {
        send(&mut editor, ch.to_string());
    }
    settle(&mut editor).await;
    assert!(editor.is_showing_autocomplete(), "expected menu for {typed}");
    send(&mut editor, "\r");
    editor.get_text()
}

#[tokio::test]
async fn applies_exact_typed_slash_argument_value_on_enter_even_when_first_item_is_highlighted() {
    let text = slash_argument_case("/argtest two", vec!["one", "two", "three"], true).await;
    assert_eq!(text, "/argtest two");
}

#[tokio::test]
async fn selects_first_prefix_match_on_enter_when_typed_arg_is_not_exact_match() {
    let text = slash_argument_case("/argtest t", vec!["two", "three", "twelve"], false).await;
    assert_eq!(text, "/argtest two");
}

#[tokio::test]
async fn highlights_unique_prefix_match_as_user_types_before_full_exact_match() {
    let text = slash_argument_case("/argtest tw", vec!["one", "two", "three"], false).await;
    assert_eq!(text, "/argtest two");
}

#[tokio::test]
async fn selects_first_prefix_match_when_multiple_items_match() {
    let text = slash_argument_case("/argtest t", vec!["one", "two", "three"], false).await;
    assert_eq!(text, "/argtest two");
}

#[tokio::test]
async fn works_for_builtin_style_command_argument_completion_path() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, _options| {
        let before = lines[0].slice(0, cursor_col);
        let Some(model_text) = before.strip_prefix("/model ") else {
            return immediate(None);
        };
        let models = ["gpt-4o", "gpt-4o-mini", "claude-sonnet"];
        let filtered: Vec<AutocompleteItem> = models
            .into_iter()
            .filter(|model| JsString::from(*model).starts_with(&model_text))
            .map(|model| AutocompleteItem::new(model, model))
            .collect();
        if filtered.is_empty() {
            return immediate(None);
        }
        immediate(Some(AutocompleteSuggestions {
            items: filtered,
            prefix: model_text.clone(),
        }))
    }));
    editor.set_autocomplete_provider(mock);
    for ch in "/model gpt-4o-mini".chars() {
        send(&mut editor, ch.to_string());
    }
    assert_eq!(editor.get_text(), "/model gpt-4o-mini");
    settle(&mut editor).await;
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\r");
    assert_eq!(editor.get_text(), "/model gpt-4o-mini");
}

#[tokio::test]
async fn awaits_async_slash_command_argument_completions() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, _options| {
        let before = lines[0].slice(0, cursor_col);
        let Some(prefix) = before.strip_prefix("/load-skills ") else {
            return immediate(None);
        };
        let prefix = prefix.clone();
        Box::pin(async move {
            tokio::task::yield_now().await;
            if prefix.starts_with('s') {
                Some(AutocompleteSuggestions {
                    items: vec![AutocompleteItem::new("skill-a", "skill-a")],
                    prefix,
                })
            } else {
                None
            }
        })
    }));
    editor.set_autocomplete_provider(mock);
    editor.set_text("/load-skills ");
    send(&mut editor, "s");
    settle(&mut editor).await;
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\t");
    assert_eq!(editor.get_text(), "/load-skills skill-a");
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn ignores_invalid_slash_command_argument_completion_results() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|_lines, _line, _cursor_col, _options| immediate(None)));
    editor.set_autocomplete_provider(mock);
    editor.set_text("/load-skills ");
    send(&mut editor, "s");
    settle(&mut editor).await;
    assert!(!editor.is_showing_autocomplete());
    assert_eq!(editor.get_text(), "/load-skills s");
}

#[tokio::test]
async fn does_not_show_argument_completions_when_command_has_no_argument_completer() {
    let (mut editor, _) = editor();
    let mut help = SlashCommand::new("help");
    help.description = Some(JsString::from("Show help"));
    let mut model = SlashCommand::new("model");
    model.description = Some(JsString::from("Switch model"));
    model.get_argument_completions = Some(Box::new(|_| {
        Some(vec![AutocompleteItem::new("claude-opus", "claude-opus")])
    }));
    let combined = Rc::new(CombinedAutocompleteProvider::new(
        vec![CommandEntry::Command(help), CommandEntry::Command(model)],
        ".",
        None,
    ));
    editor.set_autocomplete_provider(combined);
    for ch in ["/", "h", "e"] {
        send(&mut editor, ch);
    }
    settle(&mut editor).await;
    assert!(editor.is_showing_autocomplete());
    send(&mut editor, "\t");
    assert_eq!(editor.get_text(), "/help ");
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn does_not_trigger_autocomplete_during_single_line_paste() {
    let (mut editor, _) = editor();
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mock = provider(Rc::new(move |_, _, _, _| {
        counter.set(counter.get() + 1);
        immediate(None)
    }));
    editor.set_autocomplete_provider(mock);
    send(
        &mut editor,
        "\x1b[200~look at @node_modules/react/index.js please\x1b[201~",
    );
    assert_eq!(editor.get_text(), "look at @node_modules/react/index.js please");
    assert_eq!(calls.get(), 0);
    assert!(!editor.is_showing_autocomplete());
}

#[tokio::test]
async fn undoes_autocomplete() {
    let (mut editor, _) = editor();
    let mock = provider(Rc::new(|lines, _line, cursor_col, _options| {
        let prefix = lines[0].slice(0, cursor_col);
        if prefix == "di" {
            return immediate(Some(AutocompleteSuggestions {
                items: vec![AutocompleteItem::new("dist/", "dist/")],
                prefix: JsString::from("di"),
            }));
        }
        immediate(None)
    }));
    editor.set_autocomplete_provider(mock);
    send(&mut editor, "d");
    send(&mut editor, "i");
    assert_eq!(editor.get_text(), "di");
    send(&mut editor, "\t");
    settle(&mut editor).await;
    assert_eq!(editor.get_text(), "dist/");
    assert!(!editor.is_showing_autocomplete());
    send(&mut editor, "\x1b[45;5u");
    assert_eq!(editor.get_text(), "di");
}
