use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::components::markdown::{Markdown, MarkdownOptions, MarkdownTheme};
use pi_tui::tui::Component;
use serde_json::json;
struct Plain;
macro_rules! plain {($($name:ident),*) => {$ (fn $name(&self,text:&JsString)->JsString{text.clone()})*};}
impl MarkdownTheme for Plain {
    plain!(
        heading,
        link,
        link_url,
        code,
        code_block,
        code_block_border,
        quote,
        quote_border,
        hr,
        list_bullet,
        bold,
        italic,
        strikethrough,
        underline
    );
}
#[test]
fn native_markdown_raw_syntax_and_cached_lines_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 0x301, 98, 0xdc00]),
    ] {
        for (before, after) in [
            ("", ""),
            (
                "# ", "
",
            ),
            ("**", "** _x_ ~~y~~ `z`"),
            (
                "> ",
                "
> continuation",
            ),
            (
                "- ", "
- next",
            ),
            (
                "```", "
x
```",
            ),
            ("[", "a](https://example.test/a)"),
            (
                "a | ",
                "
--|--
x|y",
            ),
            ("<span>", "</span>"),
            (
                "[x]: /", "

[x]",
            ),
            ("a ", "\n\n---\n"),
        ] {
            let text = JsString::from(before) + &raw + after;
            for width in [1, 4, 12, 60] {
                let expected = oracle.request(&json!({"operation":"markdown","units":text.units(),"width":width}));
                assert_eq!(expected["ok"], true, "{expected}");
                let mut md = Markdown::new(text.clone(), 0, 0, Box::new(Plain), None, MarkdownOptions::default());
                let actual = serde_json::to_string(&[md.render(width), md.render(width)]).unwrap();
                assert_eq!(
                    parse_lossless(actual).unwrap(),
                    parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                    "text={text:?} width={width}"
                );
            }
        }
    }
}

#[test]
fn native_markdown_tokens_keep_raw_code_units_and_regex_ranges() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 0x301, 98, 0xdc00]),
    ] {
        for (before, after) in [
            ("", ""),
            ("# ", " #\n"),
            ("**", "** _x_ ~~y~~ `z`"),
            ("> ", "\n> continuation"),
            ("1) ", "\n2) next"),
            ("- [x] ", "\n- [ ] end"),
            ("```", "\nx\n```"),
            ("[", "a](https://example.test/a)"),
            ("a | ", "\n--|--\nx|y"),
            ("a\\|b | ", "\n--|--\nx|y"),
            ("<span>", "</span>"),
            ("[x]: /", "\n\n[x]"),
            ("a ", "\n\n---\n"),
            ("    ", "\n    code\n\n"),
            ("a  \n", "\n"),
            ("\\", "\n\n![alt](image.png)"),
        ] {
            let text = JsString::from(before) + &raw + after;
            let expected = oracle.request(&json!({"operation":"markdown_tokens","units":text.units()}));
            assert_eq!(expected["ok"], true, "{expected}");
            let actual = serde_json::to_string(&pi_tui::components::markdown::lexer::lex(&text)).unwrap();
            assert_eq!(
                parse_lossless(actual).unwrap(),
                parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                "text={text:?}"
            );
        }
    }
}
