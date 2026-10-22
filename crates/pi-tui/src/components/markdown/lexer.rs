//! Port of the pinned `marked` 18.0.5 lexer subset used by `markdown.ts`.
//!
//! The JavaScript grammar runs on the shared ECMAScript engine over native
//! UTF-16. Token text, masks, captures and offsets never pass through UTF-8.

use std::collections::HashMap;
use std::sync::OnceLock;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::{JsCaptures as Captures, JsRegex as Regex};

use super::tokens::{Align, LinkDefinition, ListItem, ListItemType, ListStart, TableCell, Token};

const TAG_NAMES: &str = "address|article|aside|base|basefont|blockquote|body|caption|center|col|colgroup|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|footer|form|frame|frameset|h[1-6]|head|header|hr|html|iframe|legend|li|link|main|menu|menuitem|meta|nav|noframes|ol|optgroup|option|p|param|search|section|summary|table|tbody|td|tfoot|th|thead|title|tr|track|ul";
fn rx(pattern: &str) -> Regex {
    let mut flags = "";
    let pattern = if let Some(pattern) = pattern.strip_prefix("(?i)") {
        flags = "i";
        pattern
    } else if let Some(pattern) = pattern.strip_prefix("(?m)") {
        flags = "m";
        pattern
    } else {
        pattern
    };
    // marked's property escapes are Unicode rules; its other rules use legacy units.
    let flags = if pattern.contains("\\p{") {
        if flags == "i" { "iu" } else { "u" }
    } else {
        flags
    };
    Regex::with_flags(pattern, flags).unwrap_or_else(|error| panic!("marked rule {pattern}: {error}"))
}
fn is_match(regex: &Regex, text: impl Into<JsString>) -> bool {
    regex.is_match(text)
}

struct Rules {
    // Block grammar (GFM).
    newline: Regex,
    block_code: Regex,
    fences: Regex,
    heading: Regex,
    hr: Regex,
    html_block: Regex,
    lheading: Regex,
    list: Regex,
    paragraph: Regex,
    block_text: Regex,
    def: Regex,
    blockquote: Regex,
    table: Regex,
    // Inline grammar (GFM).
    any_punctuation: Regex,
    autolink: Regex,
    block_skip: Regex,
    br: Regex,
    inline_code: Regex,
    strict_del: Regex,
    em_strong_l_delim: Regex,
    em_strong_r_delim_ast: Regex,
    em_strong_r_delim_und: Regex,
    escape: Regex,
    link: Regex,
    nolink: Regex,
    punctuation: Regex,
    reflink: Regex,
    reflink_search: Regex,
    tag: Regex,
    inline_text: Regex,
    url: Regex,
    backpedal: Regex,
    // Other helpers.
    code_remove_indent: Regex,
    indent_code_compensation: Regex,
    beginning_space: Regex,
    ending_hash: Regex,
    starting_space_char: Regex,
    ending_space_char: Regex,
    non_space_char: Regex,
    new_line_char_global: Regex,
    tab_char_global: Regex,
    multiple_space_global: Regex,
    blank_line: Regex,
    double_blank_line: Regex,
    blockquote_start: Regex,
    blockquote_setext_replace: Regex,
    blockquote_setext_replace2: Regex,
    list_is_task: Regex,
    list_replace_task: Regex,
    list_task_checkbox: Regex,
    any_line: Regex,
    href_brackets: Regex,
    table_delimiter: Regex,
    table_align_chars: Regex,
    table_row_blank_line: Regex,
    table_align_right: Regex,
    table_align_center: Regex,
    table_align_left: Regex,
    start_a_tag: Regex,
    end_a_tag: Regex,
    start_pre_script_tag: Regex,
    end_pre_script_tag: Regex,
    start_angle_bracket: Regex,
    end_angle_bracket: Regex,
    unicode_alpha_numeric: Regex,
    slash_pipe: Regex,
    carriage_return: Regex,
    output_link_replace: Regex,
    // LaTeX extensions.
}

fn gfm_paragraph_pattern() -> String {
    r"^([^\n]+(?:\n(?! {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)| {0,3}#{1,6}(?:\s|$)| {0,3}>| {0,3}(?:`{3,}(?=[^`\n]*\n)|~{3,})[^\n]*\n| {0,3}(?:[*+-]|1[.)])[ \t]+[^ \t\n]|<\/?({tags})(?: +|\n|\/?>)|<(?:script|pre|style|textarea|!--)| *([^\n ].*)\n {0,3}((?:\| *)?:?-+:? *(?:\| *:?-+:? *)*(?:\| *)?)(?:\n((?:(?! *\n| {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)| {0,3}#{1,6}(?:\s|$)| {0,3}>|(?: {4}| {0,3}\t)[^\n]| {0,3}(?:`{3,}(?=[^`\n]*\n)|~{3,})[^\n]*\n| {0,3}(?:[*+-]|1[.)])[ \t]|<\/?({tags})(?: +|\n|\/?>)|<(?:script|pre|style|textarea|!--)).*(?:\n|$))*)\n*|$)| +\n)[^\n]+)*)"
        .replace("{tags}", TAG_NAMES)
}

fn gfm_table_pattern() -> String {
    r"^ *([^\n ].*)\n {0,3}((?:\| *)?:?-+:? *(?:\| *:?-+:? *)*(?:\| *)?)(?:\n((?:(?! *\n| {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)| {0,3}#{1,6}(?:\s|$)| {0,3}>|(?: {4}| {0,3}\t)[^\n]| {0,3}(?:`{3,}(?=[^`\n]*\n)|~{3,})[^\n]*\n| {0,3}(?:[*+-]|1[.)])[ \t]|<\/?({tags})(?: +|\n|\/?>)|<(?:script|pre|style|textarea|!--)).*(?:\n|$))*)\n*|$)"
        .replace("{tags}", TAG_NAMES)
}

fn lheading_pattern(gfm: bool) -> String {
    let table = if gfm {
        r"| {0,3}\|?(?:[:\- ]*\|)+[\:\- ]*\n".to_string()
    } else {
        String::new()
    };
    r"^(?! {0,3}(?:[*+-]|\d{1,9}[.)]) |(?: {4}| {0,3}\t)| {0,3}(?:`{3,}|~{3,})| {0,3}>| {0,3}#{1,6}| {0,3}<[^\n>]+>\n{table})((?:.|\n(?!\s*?\n| {0,3}(?:[*+-]|\d{1,9}[.)]) |(?: {4}| {0,3}\t)| {0,3}(?:`{3,}|~{3,})| {0,3}>| {0,3}#{1,6}| {0,3}<[^\n>]+>\n{table}))+?)\n {0,3}(=+|-+) *(?:\n+|$)"
        .replace("{table}", &table)
}

fn html_block_pattern() -> String {
    r#"(?i)^ {0,3}(?:<(script|pre|style|textarea)[\s>][\s\S]*?(?:</\1>[^\n]*\n+|$)|<!--(?:-?>|[\s\S]*?(?:-->|$))[^\n]*(\n+|$)|<\?[\s\S]*?(?:\?>\n*|$)|<![A-Z][\s\S]*?(?:>\n*|$)|<!\[CDATA\[[\s\S]*?(?:\]\]>\n*|$)|</?({tags})(?: +|\n|/?>)[\s\S]*?(?:(?:\n[ \t]*)+\n|$)|<(?!script|pre|style|textarea)([a-z][\w-]*)(?: +[a-zA-Z:_][\w.:-]*(?: *= *"[^"\n]*"| *= *'[^'\n]*'| *= *[^\s"'=<>`]+)?)*? *\/?>(?=[ \t]*(?:\n|$))[\s\S]*?(?:(?:\n[ \t]*)+\n|$)|</(?!script|pre|style|textarea)[a-z][\w-]*\s*>(?=[ \t]*(?:\n|$))[\s\S]*?(?:(?:\n[ \t]*)+\n|$))"#
        .replace("{tags}", TAG_NAMES)
}

fn strip_carets(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut previous: Option<char> = None;
    for ch in pattern.chars() {
        if ch == '^' && previous != Some('[') {
            continue;
        }
        out.push(ch);
        previous = Some(ch);
    }
    out
}

fn blockquote_pattern() -> String {
    let paragraph = strip_carets(&gfm_paragraph_pattern());
    format!("^( {{0,3}}> ?({paragraph}|[^\\n]*)(?:\\n|$))+")
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(build_rules)
}

fn build_rules() -> Rules {
    Rules {
        newline: rx(r"^(?:[ \t]*(?:\n|$))+"),
        block_code: rx(r"^((?: {4}| {0,3}\t)[^\n]+(?:\n(?:[ \t]*(?:\n|$))*)?)+"),
        fences: rx(
            r"^ {0,3}(`{3,}(?=[^`\n]*(?:\n|$))|~{3,})([^\n]*)(?:\n|$)(?:|([\s\S]*?)(?:\n|$))(?: {0,3}\1[~`]* *(?=\n|$)|$)",
        ),
        heading: rx(r"^ {0,3}(#{1,6})(?=\s|$)(.*)(?:\n+|$)"),
        hr: rx(r"^ {0,3}((?:-[\t ]*){3,}|(?:_[ \t]*){3,}|(?:\*[ \t]*){3,})(?:\n+|$)"),
        html_block: rx(&html_block_pattern()),
        lheading: rx(&lheading_pattern(true)),
        list: rx(r"^( {0,3}(?:[*+-]|\d{1,9}[.)]))([ \t][^\n]*?)?(?:\n|$)"),
        paragraph: rx(&gfm_paragraph_pattern()),
        block_text: rx(r"^[^\n]+"),
        def: rx(
            r#"^ {0,3}\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\])+)\]: *(?:\n[ \t]*)?([^<\s][^\s]*|<.*?>)(?:(?: +(?:\n[ \t]*)?| *\n[ \t]*)((?:"(?:\\"?|[^"\\])*"|'[^'\n]*(?:\n[^'\n]+)*\n?'|\([^()]*\))))? *(?:\n+|$)"#,
        ),
        blockquote: rx(&blockquote_pattern()),
        table: rx(&gfm_table_pattern()),
        any_punctuation: rx(r"\\([\p{P}\p{S}])"),
        autolink: rx(
            r"^<([a-zA-Z][a-zA-Z0-9+.-]{1,31}:[^\s\x00-\x1f<>]*|[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+(@)[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+(?![-_]))>",
        ),
        block_skip: rx(
            r"\[(?:[^\[\]`]|(?<a>`+)[^`]+\k<a>(?!`))*?\]\((?:\\[\s\S]|[^\\\(\)]|\((?:\\[\s\S]|[^\\\(\)])*\))*\)|(?<!`)()(?<b>`+)[^`]+\k<b>(?!`)|<(?! )[^<>]*?>",
        ),
        br: rx(r"^( {2,}|\\)\n(?!\s*$)"),
        inline_code: rx(r"^(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)"),
        strict_del: rx(r"^(~~)(?=[^\s~])((?:\\.|[^\\])*?(?:\\.|[^\s~\\]))\1(?=[^~]|$)"),
        em_strong_l_delim: rx(
            r"^(?:\*+(?:((?!\*)(?!~)[\p{P}\p{S}])|([^\s*]))?)|^_+(?:((?!_)(?!~)[\p{P}\p{S}])|([^\s_]))?",
        ),
        em_strong_r_delim_ast: rx(
            r"^[^_*]*?__[^_*]*?\*[^_*]*?(?=__)|[^*]+(?=[^*])|(?!\*)(?!~)[\p{P}\p{S}](\*+)(?=[\s]|$)|(?:[^\s\p{P}\p{S}]|~)(\*+)(?!\*)(?=(?!~)[\s\p{P}\p{S}]|$)|(?!\*)(?!~)[\s\p{P}\p{S}](\*+)(?=(?:[^\s\p{P}\p{S}]|~))|[\s](\*+)(?!\*)(?=(?!~)[\p{P}\p{S}])|(?!\*)(?!~)[\p{P}\p{S}](\*+)(?!\*)(?=(?!~)[\p{P}\p{S}])|(?:[^\s\p{P}\p{S}]|~)(\*+)(?=(?:[^\s\p{P}\p{S}]|~))",
        ),
        em_strong_r_delim_und: rx(
            r"^[^_*]*?\*\*[^_*]*?_[^_*]*?(?=\*\*)|[^_]+(?=[^_])|(?!_)[\p{P}\p{S}](_+)(?=[\s]|$)|[^\s\p{P}\p{S}](_+)(?!_)(?=[\s\p{P}\p{S}]|$)|(?!_)[\s\p{P}\p{S}](_+)(?=[^\s\p{P}\p{S}])|[\s](_+)(?!_)(?=[\p{P}\p{S}])|(?!_)[\p{P}\p{S}](_+)(?!_)(?=[\p{P}\p{S}])",
        ),
        escape: rx(r##"^\\([!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])"##),
        link: rx(
            r#"^!?\[((?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\])|[^\[\]\\`])*?)\]\(\s*(<(?:\\.|[^\n<>\\])+>|[^ \t\n\x00-\x1f]*)(?:(?:[ \t]+(?:\n[ \t]*)?|\n[ \t]*)("(?:\\"?|[^"\\])*"|'(?:\\'?|[^'\\])*'|\((?:\\\)?|[^)\\])*\)))?\s*\)"#,
        ),
        nolink: rx(r"^!?\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\])+)\](?:\[\])?"),
        punctuation: rx(r"^((?![*_])[\s\p{P}\p{S}])"),
        reflink: rx(
            r"^!?\[((?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\])|[^\[\]\\`])*?)\]\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\])+)\]",
        ),
        reflink_search: rx(
            r"!?\[((?:\[(?:\\[\s\S]|[^\[\]\\])*\]|\\[\s\S]|`+(?!`)[^`]*?`+(?!`)|``+(?=\])|[^\[\]\\`])*?)\]\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\])+)\]|!?\[((?!\s*\])(?:\\[\s\S]|[^\[\]\\])+)\](?:\[\])?(?!\()",
        ),
        tag: rx(
            r#"^<!--(?:-?>|[\s\S]*?-->)|^</[a-zA-Z][\w:-]*\s*>|^<[a-zA-Z][\w-]*(?:\s+[a-zA-Z:_][\w.:-]*(?:\s*=\s*"[^"]*"|\s*=\s*'[^']*'|\s*=\s*[^\s"'=<>`]+)?)*?\s*\/?>|^<\?[\s\S]*?\?>|^<![a-zA-Z]+\s[\s\S]*?>|^<!\[CDATA\[[\s\S]*?\]\]>"#,
        ),
        inline_text: rx(
            r"^([`~]+|[^`~])(?:(?= {2,}\n)|(?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@)|[\s\S]*?(?:(?=[\\<!\[`*~_]|\b_|[hH][tT][tT][pP][sS]?|[fF][tT][pP]:\/\/|www\.|$)|[^ ](?= {2,}\n)|[^a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-](?=[a-zA-Z0-9.!#$%&'*+\/=?_`{\|}~-]+@)))",
        ),
        url: rx(
            r"^((?:[hH][tT][tT][pP][sS]?|[fF][tT][pP]):\/\/|www\.)(?:[a-zA-Z0-9\-]+\.?)+[^\s<]*|^[A-Za-z0-9._+-]+(@)[a-zA-Z0-9-_]+(?:\.[a-zA-Z0-9-_]*[a-zA-Z0-9])+(?![-_])",
        ),
        backpedal: rx(r#"(?:[^?!.,:;*_'"~()&]+|\([^)]*\)|&(?![a-zA-Z0-9]+;$)|[?!.,:;*_'"~)]+(?!$))+"#),
        code_remove_indent: rx(r"(?m)^(?: {1,4}| {0,3}\t)"),
        indent_code_compensation: rx(r"^(\s+)(?:```)"),
        beginning_space: rx(r"^\s+"),
        ending_hash: rx(r"#$"),
        starting_space_char: rx(r"^ "),
        ending_space_char: rx(r" $"),
        non_space_char: rx(r"[^ ]"),
        new_line_char_global: rx(r"\n"),
        tab_char_global: rx(r"\t"),
        multiple_space_global: rx(r"\s+"),
        blank_line: rx(r"^[ \t]*$"),
        double_blank_line: rx(r"\n[ \t]*\n[ \t]*$"),
        blockquote_start: rx(r"^ {0,3}>"),
        blockquote_setext_replace: rx(r"\n {0,3}((?:=+|-+) *)(?=\n|$)"),
        blockquote_setext_replace2: rx(r"(?m)^ {0,3}>[ \t]?"),
        list_is_task: rx(r"^\[[ xX]\] +\S"),
        list_replace_task: rx(r"^\[[ xX]\] +"),
        list_task_checkbox: rx(r"\[[ xX]\]"),
        any_line: rx(r"\n.*\n"),
        href_brackets: rx(r"^<(.*)>$"),
        table_delimiter: rx(r"[:|]"),
        table_align_chars: rx(r"^\||\| *$"),
        table_row_blank_line: rx(r"\n[ \t]*$"),
        table_align_right: rx(r"^ *-+: *$"),
        table_align_center: rx(r"^ *:-+: *$"),
        table_align_left: rx(r"^ *:-+ *$"),
        start_a_tag: rx(r"(?i)^<a "),
        end_a_tag: rx(r"(?i)^</a>"),
        start_pre_script_tag: rx(r"(?i)^<(pre|code|kbd|script)(\s|>)"),
        end_pre_script_tag: rx(r"(?i)^</(pre|code|kbd|script)(\s|>)"),
        start_angle_bracket: rx(r"^<"),
        end_angle_bracket: rx(r">$"),
        unicode_alpha_numeric: rx(r"[\p{L}\p{N}]"),
        slash_pipe: rx(r"\\\|"),
        carriage_return: rx(r"\r\n|\r"),
        output_link_replace: rx(r"\\([\[\]])"),
    }
}

fn utf16_len(value: impl Into<JsString>) -> usize {
    value.into().len()
}
fn slice_utf16(value: impl Into<JsString>, start: isize, end: Option<isize>) -> JsString {
    let value = value.into();
    let len = value.len() as isize;
    let normalize=|index:isize| if index<0 {(len+index).clamp(0,len)}else{index.clamp(0,len)} as usize;
    value.slice(normalize(start), normalize(end.unwrap_or(len)))
}
fn trim(value: impl Into<JsString>) -> JsString {
    value.into().trim()
}
fn trim_end(value: impl Into<JsString>) -> JsString {
    value.into().trim_end()
}
fn rtrim(value: impl Into<JsString>, character: char) -> JsString {
    let value = value.into();
    let end = value
        .units()
        .iter()
        .rposition(|unit| *unit != character as u16)
        .map_or(0, |index| index + 1);
    value.slice(0, end)
}
fn trim_trailing_blank_lines(value: impl Into<JsString>) -> JsString {
    let value = value.into();
    let lines: Vec<_> = value.split('\n').collect();
    let mut end = lines.len() as isize - 1;
    while end >= 0 && rules().blank_line.is_match(&lines[end as usize]) {
        end -= 1;
    }
    if lines.len() as isize - end <= 2 {
        value
    } else {
        JsString::join(lines[..(end + 1) as usize].iter(), "\n")
    }
}
fn indent_code_compensation(raw: impl Into<JsString>, text: impl Into<JsString>) -> JsString {
    let raw = raw.into();
    let text = text.into();
    let rules = rules();
    let Some(caps) = rules.indent_code_compensation.captures(&raw) else {
        return text.clone();
    };
    let indent_to_code = caps.get(1).map(|m| m.text()).unwrap_or_default();
    JsString::join(
        text.split('\n').map(|node| {
            let Some(caps) = rules.beginning_space.captures(&node) else {
                return node;
            };
            let indent = caps.get(0).map(|m| m.text()).unwrap_or_default();
            if indent.len() >= indent_to_code.len() {
                node.slice(indent_to_code.len(), node.len())
            } else {
                node
            }
        }),
        "\n",
    )
}

fn expand_tabs(line: JsString, indent: usize) -> JsString {
    let mut column = indent;
    let mut expanded = JsString::default();
    for ch in line.units().iter().copied() {
        if ch == 9 {
            let added = 4 - (column % 4);
            expanded.push_str(&" ".repeat(added));
            column += added;
        } else {
            expanded.push_unit(ch);
            column += 1;
        }
    }
    expanded
}

fn find_closing_bracket(value: &JsString, brackets: &str) -> isize {
    let open = u16::from(brackets.as_bytes()[0]);
    let close = u16::from(brackets.as_bytes()[1]);
    let bytes = value.units();
    if !bytes.contains(&close) {
        return -1;
    }
    let mut level = 0isize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == 92 {
            index += 2;
            continue;
        }
        if bytes[index] == open {
            level += 1;
        } else if bytes[index] == close {
            level -= 1;
            if level < 0 {
                return index as isize;
            }
        }
        index += 1;
    }
    if level > 0 { -2 } else { -1 }
}

fn split_cells(table_row: impl Into<JsString>, count: Option<usize>) -> Vec<JsString> {
    let table_row = table_row.into();
    let rules = rules();
    let mut spaced = JsString::default();
    for (offset, ch) in table_row.units().iter().copied().enumerate() {
        if ch != 124 {
            spaced.push_unit(ch);
            continue;
        }
        let mut escaped = false;
        let mut cursor = offset;
        while cursor > 0 {
            cursor -= 1;
            if table_row.units()[cursor] != 92 {
                break;
            }
            escaped = !escaped;
        }
        if escaped {
            spaced.push_str("|");
        } else {
            spaced.push_str(" |");
        }
    }

    let mut cells = Vec::new();
    let mut start = 0;
    while let Some(end) = spaced.find_from(" |", start) {
        cells.push(spaced.slice(start, end));
        start = end + 2;
    }
    cells.push(spaced.slice(start, spaced.len()));
    if cells.first().is_none_or(|cell| trim(cell).is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|cell| trim(cell).is_empty()) {
        cells.pop();
    }

    if let Some(count) = count {
        if cells.len() > count {
            cells.truncate(count);
        } else {
            while cells.len() < count {
                cells.push(JsString::default());
            }
        }
    }

    cells
        .into_iter()
        .map(|cell| rules.slash_pipe.replace_all(trim(&cell), "|"))
        .collect()
}

pub(super) fn type_name(token: &Token) -> &'static str {
    match token {
        Token::Space { .. } => "space",
        Token::Code { .. } => "code",
        Token::Heading { .. } => "heading",
        Token::Hr { .. } => "hr",
        Token::Blockquote { .. } => "blockquote",
        Token::List { .. } => "list",
        Token::Checkbox { .. } => "checkbox",
        Token::Html { .. } => "html",
        Token::Def { .. } => "def",
        Token::Table { .. } => "table",
        Token::Paragraph { .. } => "paragraph",
        Token::Text { .. } => "text",
        Token::Escape { .. } => "escape",
        Token::Br { .. } => "br",
        Token::Codespan { .. } => "codespan",
        Token::Em { .. } => "em",
        Token::Strong { .. } => "strong",
        Token::Del { .. } => "del",
        Token::Link { .. } => "link",
        Token::Image { .. } => "image",
    }
}

/// Lex markdown into the token stream produced by the pinned marked parser.
pub fn lex(source: &JsString) -> Vec<Token> {
    let rules = rules();
    let normalized = rules.carriage_return.replace_all(source, "\n");
    let mut lexer = Lexer::new();
    let mut tokens = Vec::new();
    lexer.block_tokens(&normalized, &mut tokens, false);
    lexer.resolve_inline(&mut tokens);
    tokens
}

struct Lexer {
    rules: &'static Rules,
    links: HashMap<JsString, LinkDefinition>,
    in_link: bool,
    in_raw_block: bool,
    top: bool,
}

impl Lexer {
    fn new() -> Self {
        Self {
            rules: rules(),
            links: HashMap::new(),
            in_link: false,
            in_raw_block: false,
            top: true,
        }
    }

    fn resolve_inline(&mut self, tokens: &mut [Token]) {
        for token in tokens.iter_mut() {
            match token {
                Token::Heading {
                    inline_source, tokens, ..
                }
                | Token::Paragraph {
                    inline_source, tokens, ..
                } => {
                    if let Some(source) = inline_source.take() {
                        *tokens = self.inline_tokens(&source);
                    }
                }
                Token::Text {
                    inline_source, tokens, ..
                } => {
                    if let Some(source) = inline_source.take() {
                        *tokens = Some(self.inline_tokens(&source));
                    }
                }
                Token::Blockquote { tokens, .. } => self.resolve_inline(tokens),
                Token::List { items, .. } => {
                    for item in items {
                        self.resolve_inline(&mut item.tokens);
                    }
                }
                Token::Table { header, rows, .. } => {
                    for cell in header {
                        if let Some(source) = cell.inline_source.take() {
                            cell.tokens = self.inline_tokens(&source);
                        }
                    }
                    for row in rows {
                        for cell in row {
                            if let Some(source) = cell.inline_source.take() {
                                cell.tokens = self.inline_tokens(&source);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn block_tokens(&mut self, source: &JsString, tokens: &mut Vec<Token>, last_paragraph_clipped: bool) {
        let mut source = source.clone();
        let mut last_paragraph_clipped = last_paragraph_clipped;
        let mut source_length = usize::MAX;

        while !source.is_empty() {
            if source.len() < source_length {
                source_length = source.len();
            } else {
                panic!("Infinite loop on byte: {}", source.units()[0]);
            }

            if let Some(token) = self.space(&source) {
                let raw_length = token.raw().len();
                source = source.slice(raw_length, source.len()).clone();
                if raw_length == 1
                    && let Some(last) = tokens.last_mut()
                {
                    last.append_raw(&"\n".into());
                } else {
                    tokens.push(token);
                }
                continue;
            }

            if let Some(token) = self.code(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                let merge = tokens
                    .last()
                    .is_some_and(|last| matches!(last, Token::Paragraph { .. } | Token::Text { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let raw = token.raw().clone();
                    let text = token.text();
                    last.merge_block(&raw, &text);
                } else {
                    tokens.push(token);
                }
                continue;
            }

            if let Some(token) = self.fences(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.heading(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.hr(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.blockquote(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.list(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.html(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.def(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                let merge = tokens
                    .last()
                    .is_some_and(|last| matches!(last, Token::Paragraph { .. } | Token::Text { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let raw = token.raw().clone();
                    last.merge_definition(&raw);
                } else {
                    use std::collections::hash_map::Entry;
                    let tag = match &token {
                        Token::Def { tag, .. } => tag.clone(),
                        _ => unreachable!(),
                    };
                    if let Entry::Vacant(entry) = self.links.entry(tag) {
                        let (href, title) = match &token {
                            Token::Def { href, title, .. } => (href.clone(), title.clone()),
                            _ => unreachable!(),
                        };
                        entry.insert(LinkDefinition { href, title });
                        tokens.push(token);
                    }
                }
                continue;
            }

            if let Some(token) = self.table(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.lheading(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            let cut_source = source.clone();
            if self.top
                && let Some(token) = self.paragraph(&cut_source)
            {
                let raw_length = token.raw().len();
                let merge = last_paragraph_clipped
                    && tokens
                        .last()
                        .is_some_and(|last| matches!(last, Token::Paragraph { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let raw = token.raw().clone();
                    let text = token.text();
                    last.merge_paragraph(&raw, &text);
                } else {
                    tokens.push(token);
                }
                last_paragraph_clipped = cut_source.len() != source.len();
                source = source.slice(raw_length, source.len()).clone();
                continue;
            }

            if let Some(token) = self.text(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                let merge = tokens.last().is_some_and(|last| matches!(last, Token::Text { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let raw = token.raw().clone();
                    let text = token.text();
                    last.merge_text(&raw, &text);
                } else {
                    tokens.push(token);
                }
                continue;
            }

            panic!("Infinite loop on byte: {}", source.units()[0]);
        }

        self.top = true;
    }

    fn space(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.newline.captures(source)?;
        let raw = caps.get(0)?.text();
        (!raw.is_empty()).then(|| Token::Space { raw: raw.clone() })
    }

    fn code(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.block_code.captures(source)?;
        let raw = trim_trailing_blank_lines(caps.get(0)?.text()).clone();
        let text = self.rules.code_remove_indent.replace_all(&raw, "");
        Some(Token::Code {
            raw,
            lang: None,
            code_block_style: Some(JsString::from("indented")),
            text,
        })
    }

    fn fences(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.fences.captures(source)?;
        let raw = caps.get(0)?.text();
        let text = indent_code_compensation(&raw, caps.get(3).map(|m| m.text()).unwrap_or_default());
        let lang = caps
            .get(2)
            .map(|m| self.rules.any_punctuation.replace_all(trim(m.text()), "$1"));
        Some(Token::Code {
            raw,
            lang,
            code_block_style: None,
            text,
        })
    }

    fn heading(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.heading.captures(source)?;
        let mut text = trim(caps.get(2)?.text()).clone();
        if is_match(&self.rules.ending_hash, &text) {
            let trimmed = rtrim(&text, '#');
            if trimmed.is_empty() || is_match(&self.rules.ending_space_char, &trimmed) {
                text = trim(trimmed).clone();
            }
        }
        let raw = rtrim(caps.get(0)?.text(), '\n').clone();
        let depth = caps.get(1)?.text().len() as u8;
        Some(Token::Heading {
            raw,
            depth,
            text: text.clone(),
            tokens: Vec::new(),
            inline_source: Some(text),
        })
    }

    fn hr(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.hr.captures(source)?;
        Some(Token::Hr {
            raw: rtrim(caps.get(0)?.text(), '\n').clone(),
        })
    }

    fn blockquote(&mut self, source: &JsString) -> Option<Token> {
        let caps = self.rules.blockquote.captures(source)?;
        let mut lines: Vec<JsString> = rtrim(caps.get(0)?.text(), '\n').split('\n').collect();
        let mut raw = JsString::default();
        let mut text = JsString::default();
        let mut tokens: Vec<Token> = Vec::new();

        while !lines.is_empty() {
            let mut in_blockquote = false;
            let mut current_lines: Vec<JsString> = Vec::new();
            let mut consumed = 0;
            for line in &lines {
                if is_match(&self.rules.blockquote_start, line) {
                    current_lines.push(line.clone());
                    in_blockquote = true;
                } else if !in_blockquote {
                    current_lines.push(line.clone());
                } else {
                    break;
                }
                consumed += 1;
            }
            lines.drain(..consumed);

            let current_raw = JsString::join(current_lines, "\n");
            let current_text = self.rules.blockquote_setext_replace2.replace_all(
                &self
                    .rules
                    .blockquote_setext_replace
                    .replace_all(&current_raw, "\n    $1"),
                "",
            );
            raw = if raw.is_empty() {
                current_raw.clone()
            } else {
                JsString::from(raw) + JsString::from("\n") + JsString::from(current_raw)
            };
            text = if text.is_empty() {
                current_text.clone()
            } else {
                JsString::from(text) + JsString::from("\n") + JsString::from(current_text.clone())
            };

            let top = self.top;
            self.top = true;
            self.block_tokens(&current_text, &mut tokens, true);
            self.top = top;

            if lines.is_empty() {
                break;
            }

            match tokens.last().map(type_name) {
                Some("code") => break,
                Some("blockquote") => {
                    let (old_raw, old_text) = match tokens.last() {
                        Some(Token::Blockquote { raw, text, .. }) => (raw.clone(), text.clone()),
                        _ => unreachable!(),
                    };
                    let new_text = JsString::from(old_raw.clone())
                        + JsString::from("\n")
                        + JsString::from(JsString::join(lines, "\n"));
                    let new_token = self.blockquote(&new_text).expect("blockquote must re-lex");
                    let new_raw = new_token.raw().clone();
                    let new_text = new_token.text();
                    raw = raw.slice(0, raw.len() - old_raw.len());
                    raw.push_str(&new_raw);
                    text = text.slice(0, text.len() - old_text.len());
                    text.push_str(&new_text);
                    *tokens.last_mut().expect("checked above") = new_token;
                    break;
                }
                Some("list") => {
                    let old_raw = match tokens.last() {
                        Some(Token::List { raw, .. }) => raw.clone(),
                        _ => unreachable!(),
                    };
                    let new_text = JsString::from(old_raw.clone())
                        + JsString::from("\n")
                        + JsString::from(JsString::join(lines, "\n"));
                    let new_token = self.list(&new_text).expect("list must re-lex");
                    let new_raw = new_token.raw().clone();
                    raw = raw.slice(0, raw.len() - old_raw.len());
                    raw.push_str(&new_raw);
                    text = text.slice(0, text.len() - old_raw.len());
                    text.push_str(&new_raw);
                    let remaining = new_text.slice(new_raw.len(), new_text.len()).clone();
                    *tokens.last_mut().expect("checked above") = new_token;
                    lines = remaining.split('\n').collect();
                    continue;
                }
                _ => {}
            }
        }

        Some(Token::Blockquote { raw, text, tokens })
    }

    fn list(&mut self, source: &JsString) -> Option<Token> {
        let caps = self.rules.list.captures(source)?;
        let bull_raw = trim(caps.get(1)?.text());
        let is_ordered = bull_raw.len() > 1;
        let start = if is_ordered {
            ListStart::Number(
                bull_raw.units()[..bull_raw.len() - 1]
                    .iter()
                    .fold(0u64, |n, u| n * 10 + u64::from(*u - 48)),
            )
        } else {
            ListStart::Empty(JsString::default())
        };
        let bull = if is_ordered {
            format!(r"\d{{1,9}}\{}", char::from_u32(u32::from(*bull_raw.units().last()?))?)
        } else {
            format!(r"\{}", char::from_u32(u32::from(bull_raw.units()[0]))?)
        };
        let item_pattern = "^( {0,3}{bull})((?:[\\t ][^\\n]*)?(?:\\n|$))".replace("{bull}", &bull);
        let item_regex = rx(&item_pattern);

        let mut source = source.clone();
        let mut list_raw = JsString::default();
        let mut loose = false;
        let mut ends_with_blank_line = false;
        let mut items: Vec<ListItem> = Vec::new();
        let mut cap = item_regex.captures(&source)?;

        while !source.is_empty() {
            let mut end_early = false;
            let mut raw = JsString::default();
            let mut item_contents = JsString::default();

            if is_match(&self.rules.hr, &source) {
                break;
            }

            let cap_raw = cap.get(0)?.text();
            let cap1 = cap.get(1)?.text();
            let cap2 = cap.get(2)?.text();
            raw.push_str(&cap_raw);
            source = source.slice(cap_raw.len(), source.len()).clone();

            let first_line = cap2.split('\n').next().unwrap_or_default();
            let mut line = expand_tabs(first_line, cap1.len());
            let next_line = source.split('\n').next().unwrap_or_default().clone();
            let mut blank_line = trim(&line).is_empty();

            let mut indent: isize;
            if blank_line {
                indent = cap1.len() as isize + 1;
            } else {
                let search = line
                    .units()
                    .iter()
                    .position(|ch| *ch != 32)
                    .map(|i| i as isize)
                    .unwrap_or(-1);
                indent = if search > 4 { 1 } else { search };
                let content_start = usize::try_from(indent).unwrap_or(0);
                item_contents = slice_utf16(&line, content_start as isize, None).clone();
                indent += cap1.len() as isize;
            }

            if blank_line && is_match(&self.rules.blank_line, &next_line) {
                raw.push_str(&next_line);
                raw.push_str("\n");
                let consumed = (next_line.len() + 1).min(source.len());
                source = source.slice(consumed, source.len()).clone();
                end_early = true;
            }

            if !end_early {
                let indent_usize = usize::try_from(indent).unwrap_or(0);
                let indent_regex = indent_usize.saturating_sub(1).min(3);
                let next_bullet = rx(&"^ {0,{indent}}(?:[*+-]|\\d{1,9}[.)])((?:[ \\t][^\\n]*)?(?:\\n|$))"
                    .replace("{indent}", &indent_regex.to_string()));
                let hr_regex = rx(&"^ {0,{indent}}((?:- *){3,}|(?:_ *){3,}|(?:\\* *){3,})(?:\\n+|$)"
                    .replace("{indent}", &indent_regex.to_string()));
                let fences_begin = rx(&"^ {0,{indent}}(?:```|~~~)".replace("{indent}", &indent_regex.to_string()));
                let heading_begin = rx(&"^ {0,{indent}}#".replace("{indent}", &indent_regex.to_string()));
                let html_begin =
                    rx(&"(?i)^ {0,{indent}}<(?:[a-z].*>|!--)".replace("{indent}", &indent_regex.to_string()));
                let blockquote_begin = rx(&"^ {0,{indent}}>".replace("{indent}", &indent_regex.to_string()));

                while !source.is_empty() {
                    let raw_line = source.split('\n').next().unwrap_or_default().clone();
                    let mut next = raw_line.clone();
                    let next_without_tabs = self.rules.tab_char_global.replace_all(&next, "    ");
                    next = raw_line.clone();

                    if is_match(&fences_begin, &next)
                        || is_match(&heading_begin, &next)
                        || is_match(&html_begin, &next)
                        || is_match(&blockquote_begin, &next)
                        || is_match(&next_bullet, &next)
                        || is_match(&hr_regex, &next)
                    {
                        break;
                    }

                    let search = next_without_tabs
                        .units()
                        .iter()
                        .position(|ch| *ch != 32)
                        .map(|i| i as isize)
                        .unwrap_or(-1);
                    if search >= indent || trim(&next).is_empty() {
                        item_contents.push_str("\n");
                        item_contents.push_str(slice_utf16(&next_without_tabs, indent, None));
                    } else {
                        if blank_line {
                            break;
                        }
                        let line_with_tabs = self.rules.tab_char_global.replace_all(&line, "    ");
                        let line_search = line_with_tabs
                            .units()
                            .iter()
                            .position(|ch| *ch != 32)
                            .map(|i| i as isize)
                            .unwrap_or(-1);
                        if line_search >= 4
                            || is_match(&fences_begin, &line)
                            || is_match(&heading_begin, &line)
                            || is_match(&hr_regex, &line)
                        {
                            break;
                        }
                        item_contents.push_str("\n");
                        item_contents.push_str(&next);
                    }

                    blank_line = trim(&next).is_empty();
                    raw.push_str(&raw_line);
                    raw.push_str("\n");
                    let consumed = (raw_line.len() + 1).min(source.len());
                    source = source.slice(consumed, source.len()).clone();
                    line = slice_utf16(&next_without_tabs, indent, None).clone();
                }
            }

            if !loose {
                if ends_with_blank_line {
                    loose = true;
                } else if is_match(&self.rules.double_blank_line, &raw) {
                    ends_with_blank_line = true;
                }
            }

            items.push(ListItem {
                token_type: ListItemType::ListItem,
                raw: raw.clone(),
                task: is_match(&self.rules.list_is_task, &item_contents),
                checked: None,
                loose: false,
                text: item_contents,
                tokens: Vec::new(),
            });
            list_raw.push_str(&raw);

            cap = match item_regex.captures(&source) {
                Some(cap) => cap,
                None => break,
            };
        }

        let last = items.last_mut()?;
        last.raw = trim_end(&last.raw).clone();
        last.text = trim_end(&last.text).clone();
        list_raw = trim_end(&list_raw).clone();

        for item in &mut items {
            self.top = false;
            let mut item_tokens = Vec::new();
            self.block_tokens(&item.text.clone(), &mut item_tokens, false);
            item.tokens = item_tokens;

            if item.task
                && item
                    .tokens
                    .first()
                    .is_some_and(|token| matches!(token, Token::Text { .. } | Token::Paragraph { .. }))
            {
                item.text = self.rules.list_replace_task.replace_all(&item.text, "");
                let first = item.tokens.first_mut().expect("checked above");
                first.replace_raw_and_text(&self.rules.list_replace_task);
                replace_last_inline_task(&mut item.tokens, &self.rules.list_replace_task);

                if let Some(task_raw) = self.rules.list_task_checkbox.find(&item.raw) {
                    let checkbox_raw = JsString::from(task_raw.text()) + JsString::from(" ");
                    let checked = task_raw.text() != "[ ]";
                    item.checked = Some(checked);
                    let checkbox = Token::Checkbox {
                        raw: checkbox_raw.clone(),
                        checked,
                    };
                    if loose {
                        let first = item.tokens.first_mut().expect("checked above");
                        match first {
                            Token::Paragraph { raw, text, tokens, .. }
                            | Token::Text {
                                raw,
                                text,
                                tokens: Some(tokens),
                                ..
                            } => {
                                *raw = checkbox_raw.clone() + &*raw;
                                *text = checkbox_raw.clone() + &*text;
                                tokens.insert(0, checkbox);
                            }
                            _ => {
                                item.tokens.insert(
                                    0,
                                    Token::Paragraph {
                                        raw: checkbox_raw.clone(),
                                        text: checkbox_raw.clone(),
                                        tokens: vec![checkbox],
                                        inline_source: None,
                                    },
                                );
                            }
                        }
                    } else {
                        item.tokens.insert(0, checkbox);
                    }
                }
            } else if item.task {
                item.task = false;
            }

            if !loose {
                let spacers: Vec<&Token> = item
                    .tokens
                    .iter()
                    .filter(|token| matches!(token, Token::Space { .. }))
                    .collect();
                loose = !spacers.is_empty() && spacers.iter().any(|token| is_match(&self.rules.any_line, token.raw()));
            }
        }

        if loose {
            for item in &mut items {
                item.loose = true;
                for token in &mut item.tokens {
                    if matches!(token, Token::Text { .. }) {
                        let replacement = match std::mem::replace(
                            token,
                            Token::Space {
                                raw: JsString::default(),
                            },
                        ) {
                            Token::Text {
                                raw,
                                text,
                                tokens,
                                inline_source,
                                ..
                            } => Token::Paragraph {
                                raw,
                                text,
                                tokens: tokens.unwrap_or_default(),
                                inline_source,
                            },
                            other => other,
                        };
                        *token = replacement;
                    }
                }
            }
        }

        Some(Token::List {
            raw: list_raw,
            ordered: is_ordered,
            start,
            loose,
            items,
        })
    }

    fn html(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.html_block.captures(source)?;
        let raw = trim_trailing_blank_lines(caps.get(0)?.text()).clone();
        let group = caps.get(1).map(|m| m.text()).unwrap_or_default();
        let pre = group == "pre" || group == "script" || group == "style";
        Some(Token::Html {
            raw: raw.clone(),
            block: true,
            pre: Some(pre),
            in_link: None,
            in_raw_block: None,
            text: raw,
        })
    }

    fn def(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.def.captures(source)?;
        let tag = self
            .rules
            .multiple_space_global
            .replace_all(&caps.get(1)?.text().to_lowercase(), " ");
        let href = caps
            .get(2)
            .map(|m| {
                self.rules
                    .any_punctuation
                    .replace_all(&self.rules.href_brackets.replace_all(m.text(), "$1"), "$1")
            })
            .unwrap_or_default();
        let title = caps.get(3).map(|m| {
            let value = m.text();
            self.rules
                .any_punctuation
                .replace_all(value.slice(1, value.len() - 1), "$1")
        });
        Some(Token::Def {
            raw: rtrim(caps.get(0)?.text(), '\n').clone(),
            tag,
            href,
            title,
        })
    }

    fn table(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.table.captures(source)?;
        let align_source = caps.get(2)?.text();
        if !is_match(&self.rules.table_delimiter, &align_source) {
            return None;
        }

        let headers = split_cells(caps.get(1)?.text(), None);
        let align_source_stripped = self.rules.table_align_chars.replace_all(align_source, "");
        let aligns: Vec<JsString> = align_source_stripped.split('|').collect();
        let rows_source = caps.get(3).map(|m| m.text()).unwrap_or_default();
        let rows_stripped = self.rules.table_row_blank_line.replace_all(&rows_source, "");
        let rows: Vec<JsString> = if trim(rows_source).is_empty() {
            Vec::new()
        } else {
            rows_stripped.split('\n').collect()
        };

        if headers.len() != aligns.len() {
            return None;
        }

        let mut align = Vec::with_capacity(aligns.len());
        for value in &aligns {
            if is_match(&self.rules.table_align_right, value) {
                align.push(Some(Align::Right));
            } else if is_match(&self.rules.table_align_center, value) {
                align.push(Some(Align::Center));
            } else if is_match(&self.rules.table_align_left, value) {
                align.push(Some(Align::Left));
            } else {
                align.push(None);
            }
        }

        let header: Vec<TableCell> = headers
            .into_iter()
            .enumerate()
            .map(|(index, text)| TableCell {
                text: text.clone(),
                tokens: Vec::new(),
                header: true,
                align: align[index],
                inline_source: Some(text),
            })
            .collect();

        let table_rows: Vec<Vec<TableCell>> = rows
            .into_iter()
            .map(|row| {
                split_cells(row, Some(header.len()))
                    .into_iter()
                    .enumerate()
                    .map(|(index, text)| TableCell {
                        text: text.clone(),
                        tokens: Vec::new(),
                        header: false,
                        align: align[index],
                        inline_source: Some(text),
                    })
                    .collect()
            })
            .collect();

        Some(Token::Table {
            raw: rtrim(caps.get(0)?.text(), '\n').clone(),
            header,
            align,
            rows: table_rows,
        })
    }

    fn lheading(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.lheading.captures(source)?;
        let text = trim(caps.get(1)?.text()).clone();
        let marker = caps.get(2)?.text();
        Some(Token::Heading {
            raw: rtrim(caps.get(0)?.text(), '\n').clone(),
            depth: if marker.starts_with('=') { 1 } else { 2 },
            text: text.clone(),
            tokens: Vec::new(),
            inline_source: Some(text),
        })
    }

    fn paragraph(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.paragraph.captures(source)?;
        let value = caps.get(1)?.text();
        let text = value.strip_suffix('\n').unwrap_or(value).clone();
        Some(Token::Paragraph {
            raw: caps.get(0)?.text(),
            text: text.clone(),
            tokens: Vec::new(),
            inline_source: Some(text),
        })
    }

    fn text(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.block_text.captures(source)?;
        let raw = caps.get(0)?.text();
        Some(Token::Text {
            raw: raw.clone(),
            text: raw.clone(),
            escaped: None,
            tokens: None,
            inline_source: Some(raw),
        })
    }

    fn inline_tokens(&mut self, source: &JsString) -> Vec<Token> {
        let mut masked = source.clone();
        if !self.links.is_empty() {
            masked = mask_reflinks(masked, &self.rules.reflink_search, &self.links);
        }
        masked = mask_global(masked, &self.rules.any_punctuation, |_| Some(JsString::from("++")));
        masked = mask_global(masked, &self.rules.block_skip, |caps| {
            let length = caps.get(0).map(|m| utf16_len(m.text())).unwrap_or(0);
            let offset = caps.get(2).map(|m| utf16_len(m.text())).unwrap_or(0);
            let repeated = length.saturating_sub(offset + 2);
            Some(JsString::from("[") + JsString::from("a".repeat(repeated)) + JsString::from("]"))
        });

        let mut source = source.clone();
        let mut tokens: Vec<Token> = Vec::new();
        let mut prev_char = JsString::default();
        let mut keep_prev_char = false;

        while !source.is_empty() {
            if !keep_prev_char {
                prev_char = JsString::default();
            }
            keep_prev_char = false;

            if let Some(token) = self.escape(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.tag(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.link(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.reflink(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                let merge = matches!(token, Token::Text { .. })
                    && tokens.last().is_some_and(|last| matches!(last, Token::Text { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let raw = token.raw().clone();
                    let text = token.text();
                    last.merge_inline_text(&raw, &text);
                } else {
                    tokens.push(token);
                }
                continue;
            }

            if let Some(token) = self.em_strong(&source, &masked, &prev_char) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.codespan(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.br(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.del(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if let Some(token) = self.autolink(&source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            if !self.in_link
                && let Some(token) = self.url(&source)
            {
                source = source.slice(token.raw().len(), source.len()).clone();
                tokens.push(token);
                continue;
            }

            let cut_source = clip_inline_text(&source);
            if let Some(token) = self.inline_text(&cut_source) {
                source = source.slice(token.raw().len(), source.len()).clone();
                let raw = token.raw().clone();
                if !raw.ends_with('_')
                    && let Some(last) = raw.units().last().copied()
                {
                    prev_char = JsString::from_utf16(vec![last]);
                }
                keep_prev_char = true;
                let merge = tokens.last().is_some_and(|last| matches!(last, Token::Text { .. }));
                if merge {
                    let last = tokens.last_mut().expect("checked above");
                    let text = token.text();
                    last.merge_inline_text(&raw, &text);
                } else {
                    tokens.push(token);
                }
                continue;
            }

            panic!("Infinite loop on byte: {}", source.units()[0]);
        }

        tokens
    }

    fn escape(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.escape.captures(source)?;
        Some(Token::Escape {
            raw: caps.get(0)?.text(),
            text: caps.get(1)?.text(),
        })
    }

    fn tag(&mut self, source: &JsString) -> Option<Token> {
        let caps = self.rules.tag.captures(source)?;
        let raw = caps.get(0)?.text();
        if !self.in_link && is_match(&self.rules.start_a_tag, &raw) {
            self.in_link = true;
        } else if self.in_link && is_match(&self.rules.end_a_tag, &raw) {
            self.in_link = false;
        }
        if !self.in_raw_block && is_match(&self.rules.start_pre_script_tag, &raw) {
            self.in_raw_block = true;
        } else if self.in_raw_block && is_match(&self.rules.end_pre_script_tag, &raw) {
            self.in_raw_block = false;
        }
        Some(Token::Html {
            raw: raw.clone(),
            block: false,
            pre: None,
            in_link: Some(self.in_link),
            in_raw_block: Some(self.in_raw_block),
            text: raw,
        })
    }

    fn link(&mut self, source: &JsString) -> Option<Token> {
        let caps = self.rules.link.captures(source)?;
        let mut raw = caps.get(0)?.text();
        let label = caps.get(1)?.text();
        let mut href = caps.get(2)?.text();
        let mut title = caps.get(3).map(|m| m.text());

        let trimmed_url = trim(&href).clone();
        if is_match(&self.rules.start_angle_bracket, &trimmed_url) {
            if !is_match(&self.rules.end_angle_bracket, &trimmed_url) {
                return None;
            }
            let without_close = slice_utf16(&trimmed_url, 0, Some(-1));
            let rtrim_slash = rtrim(without_close, '\\');
            if (utf16_len(&trimmed_url) - utf16_len(rtrim_slash)).is_multiple_of(2) {
                return None;
            }
        } else {
            let last_paren = find_closing_bracket(&href, "()");
            if last_paren == -2 {
                return None;
            }
            if last_paren > -1 {
                let start = if raw.starts_with('!') { 5 } else { 4 };
                let link_len = start + utf16_len(&label) + last_paren as usize;
                href = slice_utf16(&href, 0, Some(last_paren)).clone();
                raw = trim(slice_utf16(&raw, 0, Some(link_len as isize))).clone();
                title = None;
            }
        }

        let mut href = trim(&href).clone();
        if is_match(&self.rules.start_angle_bracket, &href) {
            href = slice_utf16(&href, 1, Some(-1)).clone();
        }
        let title = title.map(|value| slice_utf16(&value, 1, Some(-1)).clone());
        Some(self.output_link(&raw, &label, href, title))
    }

    fn reflink(&mut self, source: &JsString) -> Option<Token> {
        let caps = match self.rules.reflink.captures(source) {
            Some(caps) => caps,
            None => self.rules.nolink.captures(source)?,
        };
        let link_string = self
            .rules
            .multiple_space_global
            .replace_all(
                caps.get(2)
                    .or_else(|| caps.get(1))
                    .map(|m| m.text())
                    .unwrap_or_default(),
                " ",
            )
            .to_lowercase();
        let Some(link) = self.links.get(&link_string).cloned() else {
            let raw = caps.get(0)?.text().slice(0, 1);
            return Some(Token::Text {
                raw: raw.clone(),
                text: raw,
                escaped: None,
                tokens: None,
                inline_source: None,
            });
        };
        Some(self.output_link(&caps.get(0)?.text(), &caps.get(1)?.text(), link.href, link.title))
    }

    fn output_link(&mut self, raw: &JsString, label: &JsString, href: JsString, title: Option<JsString>) -> Token {
        let text = self.rules.output_link_replace.replace_all(label, "$1");
        let href = if href.is_empty() {
            href
        } else {
            self.rules.any_punctuation.replace_all(&href, "$1")
        };
        let title = title
            .filter(|value| !value.is_empty())
            .map(|value| self.rules.any_punctuation.replace_all(&value, "$1"));
        self.in_link = true;
        let tokens = self.inline_tokens(&text);
        self.in_link = false;
        if raw.starts_with('!') {
            Token::Image {
                raw: raw.clone(),
                href,
                title: Some(title),
                text,
                tokens,
            }
        } else {
            Token::Link {
                raw: raw.clone(),
                href,
                title: Some(title),
                text,
                tokens,
            }
        }
    }

    fn em_strong(&mut self, source: &JsString, masked: &JsString, prev_char: &JsString) -> Option<Token> {
        let caps = self.rules.em_strong_l_delim.captures(source)?;
        if caps.get(1).is_none() && caps.get(2).is_none() && caps.get(3).is_none() && caps.get(4).is_none() {
            return None;
        }
        if caps.get(4).is_some() && is_match(&self.rules.unicode_alpha_numeric, prev_char) {
            return None;
        }

        let next_char = caps
            .get(1)
            .or_else(|| caps.get(3))
            .map(|m| m.text())
            .unwrap_or_default();
        if !next_char.is_empty() && !prev_char.is_empty() && !is_match(&self.rules.punctuation, prev_char) {
            return None;
        }

        let matched = caps.get(0)?.text();
        let l_length = char::decode_utf16(matched.units().iter().copied()).count() - 1;
        let mut delim_total = l_length as isize;
        let mut mid_delim_total = 0isize;
        let end_regex = if matched.starts_with('*') {
            &self.rules.em_strong_r_delim_ast
        } else {
            &self.rules.em_strong_r_delim_und
        };

        let sliced = slice_utf16(masked, -(utf16_len(source) as isize) + l_length as isize, None);
        let mut cursor = 0usize;
        loop {
            let start_unit = cursor;
            let caps = end_regex.captures_from(&sliced, start_unit)?;
            let matched = caps.get(0)?;
            let match_index = matched.start();
            let match_end = match_index + utf16_len(matched.text());
            let r_delim = (1..=6).find_map(|index| caps.get(index).map(|m| m.text()));
            let Some(r_delim) = r_delim else {
                cursor = match_end;
                continue;
            };
            let mut r_length = r_delim.len();

            if caps.get(3).is_some() || caps.get(4).is_some() {
                delim_total += r_length as isize;
                cursor = match_end;
                continue;
            } else if (caps.get(5).is_some() || caps.get(6).is_some())
                && l_length % 3 != 0
                && (l_length + r_length) % 3 == 0
            {
                mid_delim_total += r_length as isize;
                cursor = match_end;
                continue;
            }

            delim_total -= r_length as isize;
            if delim_total > 0 {
                cursor = match_end;
                continue;
            }

            r_length = (r_length as isize).min(r_length as isize + delim_total + mid_delim_total) as usize;
            let matched_text = matched.text();
            let last_char_length = char::decode_utf16(matched_text.units().iter().copied())
                .next()?
                .map_or(1, char::len_utf16);
            let raw_end = l_length as isize + match_index as isize + last_char_length as isize + r_length as isize;
            let raw = slice_utf16(source, 0, Some(raw_end)).clone();

            if l_length.min(r_length) % 2 == 1 {
                let text = slice_utf16(&raw, 1, Some(-1)).clone();
                let tokens = self.inline_tokens(&text);
                return Some(Token::Em { raw, text, tokens });
            }
            let text = slice_utf16(&raw, 2, Some(-2)).clone();
            let tokens = self.inline_tokens(&text);
            return Some(Token::Strong { raw, text, tokens });
        }
    }

    fn codespan(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.inline_code.captures(source)?;
        let mut text = self.rules.new_line_char_global.replace_all(caps.get(2)?.text(), " ");
        let has_non_space = is_match(&self.rules.non_space_char, &text);
        let both_ends =
            is_match(&self.rules.starting_space_char, &text) && is_match(&self.rules.ending_space_char, &text);
        if has_non_space && both_ends {
            text = slice_utf16(&text, 1, Some(-1)).clone();
        }
        Some(Token::Codespan {
            raw: caps.get(0)?.text(),
            text,
        })
    }

    fn br(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.br.captures(source)?;
        Some(Token::Br {
            raw: caps.get(0)?.text(),
        })
    }

    fn del(&mut self, source: &JsString) -> Option<Token> {
        let caps = self.rules.strict_del.captures(source)?;
        let text = caps.get(2)?.text();
        let tokens = self.inline_tokens(&text);
        Some(Token::Del {
            raw: caps.get(0)?.text(),
            text,
            tokens,
        })
    }

    fn autolink(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.autolink.captures(source)?;
        let value = caps.get(1)?.text();
        let (text, href) = if caps.get(2).map(|m| m.text()) == Some(JsString::from("@")) {
            (value.clone(), (JsString::from("mailto:") + &value))
        } else {
            (value.clone(), value.clone())
        };
        Some(Token::Link {
            raw: caps.get(0)?.text(),
            href,
            title: None,
            text: text.clone(),
            tokens: vec![Token::Text {
                raw: text.clone(),
                text,
                escaped: None,
                tokens: None,
                inline_source: None,
            }],
        })
    }

    fn url(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.url.captures(source)?;
        let mut value = caps.get(0)?.text();
        let (text, href) = if caps.get(2).map(|m| m.text()) == Some(JsString::from("@")) {
            (value.clone(), (JsString::from("mailto:") + &value))
        } else {
            loop {
                let previous = value.clone();
                value = self
                    .rules
                    .backpedal
                    .captures(&value)
                    .and_then(|caps| caps.get(0).map(|m| m.text()))
                    .unwrap_or_default();
                if previous == value {
                    break;
                }
            }
            let prefix = caps.get(1).map(|m| m.text()).unwrap_or_default();
            let href = if prefix == "www." {
                JsString::from("http://") + &value
            } else {
                value.clone()
            };
            (value.clone(), href)
        };
        Some(Token::Link {
            raw: value.clone(),
            href,
            title: None,
            text: text.clone(),
            tokens: vec![Token::Text {
                raw: text.clone(),
                text,
                escaped: None,
                tokens: None,
                inline_source: None,
            }],
        })
    }

    fn inline_text(&self, source: &JsString) -> Option<Token> {
        let caps = self.rules.inline_text.captures(source)?;
        let raw = caps.get(0)?.text();
        Some(Token::Text {
            raw: raw.clone(),
            text: raw,
            escaped: Some(self.in_raw_block),
            tokens: None,
            inline_source: None,
        })
    }
}

fn clip_inline_text(source: &JsString) -> JsString {
    let temp = slice_utf16(source, 1, None);
    let mut start: Option<usize> = None;
    for needle in ["$", "\\(", "\\["] {
        if let Some(byte) = temp.find(needle) {
            let index = byte;
            start = Some(start.map_or(index, |current: usize| current.min(index)));
        }
    }
    match start {
        Some(start) => slice_utf16(source, 0, Some(start as isize + 1)).clone(),
        None => source.clone(),
    }
}

fn mask_global(
    mut text: JsString,
    regex: &Regex,
    mut replacement: impl FnMut(&Captures) -> Option<JsString>,
) -> JsString {
    let mut cursor = 0usize;
    loop {
        let start_unit = cursor;
        let Some(caps) = regex.captures_from(&text, start_unit) else {
            break;
        };
        let Some(matched) = caps.get(0) else {
            break;
        };
        let match_start = matched.start();
        let match_end = match_start + utf16_len(matched.text());
        match replacement(&caps) {
            Some(value) => {
                let next = JsString::from(text.slice(0, matched.start()))
                    + JsString::from(&value)
                    + JsString::from(text.slice(matched.end(), text.len()));
                text = next;
                cursor = match_start + utf16_len(&value);
            }
            None => {
                cursor = match_end;
            }
        }
        if match_start == match_end {
            if cursor >= utf16_len(&text) {
                break;
            }
            cursor += 1;
        }
    }
    text
}

fn mask_reflinks(text: JsString, regex: &Regex, links: &HashMap<JsString, LinkDefinition>) -> JsString {
    mask_global(text, regex, |caps| {
        let raw = caps.get(0)?.text();
        let open = raw.units().iter().rposition(|unit| *unit == 91)?;
        let open_utf16 = open;
        let label = slice_utf16(&raw, open_utf16 as isize + 1, Some(-1));
        if !links.contains_key(&label) {
            return None;
        }
        Some(JsString::from("[") + JsString::from("a".repeat(utf16_len(raw).saturating_sub(2))) + JsString::from("]"))
    })
}

fn replace_last_inline_task(tokens: &mut [Token], task_regex: &Regex) {
    for token in tokens.iter_mut().rev() {
        let inline_source = match token {
            Token::Heading { inline_source, .. }
            | Token::Paragraph { inline_source, .. }
            | Token::Text { inline_source, .. } => inline_source,
            Token::Blockquote { tokens, .. } => {
                replace_last_inline_task(tokens, task_regex);
                continue;
            }
            Token::List { items, .. } => {
                for item in items.iter_mut().rev() {
                    replace_last_inline_task(&mut item.tokens, task_regex);
                }
                continue;
            }
            _ => continue,
        };
        if let Some(source) = inline_source
            && is_match(task_regex, &*source)
        {
            *inline_source = Some(task_regex.replace_all(&*source, ""));
            return;
        }
    }
}

impl Token {
    fn raw(&self) -> &JsString {
        match self {
            Token::Space { raw }
            | Token::Code { raw, .. }
            | Token::Heading { raw, .. }
            | Token::Hr { raw }
            | Token::Blockquote { raw, .. }
            | Token::List { raw, .. }
            | Token::Checkbox { raw, .. }
            | Token::Html { raw, .. }
            | Token::Def { raw, .. }
            | Token::Table { raw, .. }
            | Token::Paragraph { raw, .. }
            | Token::Text { raw, .. }
            | Token::Escape { raw, .. }
            | Token::Br { raw }
            | Token::Codespan { raw, .. }
            | Token::Em { raw, .. }
            | Token::Strong { raw, .. }
            | Token::Del { raw, .. }
            | Token::Link { raw, .. }
            | Token::Image { raw, .. } => raw,
        }
    }

    fn text(&self) -> JsString {
        self.has_text().cloned().unwrap_or_default()
    }

    fn append_raw(&mut self, suffix: &JsString) {
        match self {
            Token::Space { raw }
            | Token::Code { raw, .. }
            | Token::Heading { raw, .. }
            | Token::Hr { raw }
            | Token::Blockquote { raw, .. }
            | Token::List { raw, .. }
            | Token::Checkbox { raw, .. }
            | Token::Html { raw, .. }
            | Token::Def { raw, .. }
            | Token::Table { raw, .. }
            | Token::Paragraph { raw, .. }
            | Token::Text { raw, .. }
            | Token::Escape { raw, .. }
            | Token::Br { raw }
            | Token::Codespan { raw, .. }
            | Token::Em { raw, .. }
            | Token::Strong { raw, .. }
            | Token::Del { raw, .. }
            | Token::Link { raw, .. }
            | Token::Image { raw, .. } => raw.push_str(suffix),
        }
    }

    fn merge_block(&mut self, raw: &JsString, text: &JsString) {
        let separator = if self.raw().ends_with('\n') { "" } else { "\n" };
        let appended = JsString::from(separator) + JsString::from(raw);
        self.append_raw(&appended);
        match self {
            Token::Paragraph {
                text: current,
                inline_source,
                ..
            } => {
                current.push_str("\n");
                current.push_str(text);
                *inline_source = Some(current.clone());
            }
            Token::Text {
                text: current,
                inline_source,
                ..
            } => {
                current.push_str("\n");
                current.push_str(text);
                *inline_source = Some(current.clone());
            }
            _ => {}
        }
    }

    fn merge_definition(&mut self, raw: &JsString) {
        let separator = if self.raw().ends_with('\n') { "" } else { "\n" };
        let appended = JsString::from(separator) + JsString::from(raw);
        self.append_raw(&appended);
        match self {
            Token::Paragraph {
                text, inline_source, ..
            }
            | Token::Text {
                text, inline_source, ..
            } => {
                text.push_str("\n");
                text.push_str(raw);
                *inline_source = Some(text.clone());
            }
            _ => {}
        }
    }

    fn merge_paragraph(&mut self, raw: &JsString, text: &JsString) {
        let separator = if self.raw().ends_with('\n') { "" } else { "\n" };
        let appended = JsString::from(separator) + JsString::from(raw);
        self.append_raw(&appended);
        if let Token::Paragraph {
            text: current,
            inline_source,
            ..
        } = self
        {
            current.push_str("\n");
            current.push_str(text);
            *inline_source = Some(current.clone());
        }
    }

    fn merge_text(&mut self, raw: &JsString, text: &JsString) {
        let separator = if self.raw().ends_with('\n') { "" } else { "\n" };
        let appended = JsString::from(separator) + JsString::from(raw);
        self.append_raw(&appended);
        if let Token::Text {
            text: current,
            inline_source,
            ..
        } = self
        {
            current.push_str("\n");
            current.push_str(text);
            *inline_source = Some(current.clone());
        }
    }

    fn merge_inline_text(&mut self, raw: &JsString, text: &JsString) {
        self.append_raw(raw);
        if let Token::Text { text: current, .. } = self {
            current.push_str(text);
        }
    }

    fn replace_raw_and_text(&mut self, regex: &Regex) {
        match self {
            Token::Paragraph { raw, text, .. } | Token::Text { raw, text, .. } => {
                *raw = regex.replace_all(&*raw, "");
                *text = regex.replace_all(&*text, "");
            }
            _ => {}
        }
    }
}
