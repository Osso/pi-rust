//! Pure helpers from the pinned plan-mode example. No tool execution.
use fancy_regex::Regex;
use pi_js_compat::string::{js_trim, utf16_len, utf16_slice_to_utf8};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TodoItem {
    pub step: usize,
    pub text: String,
    pub completed: bool,
}

// JavaScript uses ASCII word boundaries but Unicode whitespace, including BOM.
fn js_regex(pattern: &str) -> Regex {
    let pattern = pattern
        .replace(
            r"\b",
            r"(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))",
        )
        .replace(
            r"\s",
            r"[\t\n\x0b\x0c\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]",
        );
    Regex::new(&pattern).expect("constant JS-compatible pattern")
}
fn matches(pattern: &Regex, text: &str) -> bool {
    pattern.is_match(text).expect("bounded example regex")
}
static DESTRUCTIVE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)\brm\b",
        r"(?i)\brmdir\b",
        r"(?i)\bmv\b",
        r"(?i)\bcp\b",
        r"(?i)\bmkdir\b",
        r"(?i)\btouch\b",
        r"(?i)\bchmod\b",
        r"(?i)\bchown\b",
        r"(?i)\bchgrp\b",
        r"(?i)\bln\b",
        r"(?i)\btee\b",
        r"(?i)\btruncate\b",
        r"(?i)\bdd\b",
        r"(?i)\bshred\b",
        r"(^|[^<])>(?!>)",
        r">>",
        r"(?i)\bnpm\s+(install|uninstall|update|ci|link|publish)",
        r"(?i)\byarn\s+(add|remove|install|publish)",
        r"(?i)\bpnpm\s+(add|remove|install|publish)",
        r"(?i)\bpip\s+(install|uninstall)",
        r"(?i)\bapt(-get)?\s+(install|remove|purge|update|upgrade)",
        r"(?i)\bbrew\s+(install|uninstall|upgrade)",
        r"(?i)\bgit\s+(add|commit|push|pull|merge|rebase|reset|checkout|branch\s+-[dD]|stash|cherry-pick|revert|tag|init|clone)",
        r"(?i)\bsudo\b",
        r"(?i)\bsu\b",
        r"(?i)\bkill\b",
        r"(?i)\bpkill\b",
        r"(?i)\bkillall\b",
        r"(?i)\breboot\b",
        r"(?i)\bshutdown\b",
        r"(?i)\bsystemctl\s+(start|stop|restart|enable|disable)",
        r"(?i)\bservice\s+\S+\s+(start|stop|restart)",
        r"(?i)\b(vim?|nano|emacs|code|subl)\b"
    ].map(js_regex).into()
});
static SAFE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^\s*cat\b",
        r"^\s*head\b",
        r"^\s*tail\b",
        r"^\s*less\b",
        r"^\s*more\b",
        r"^\s*grep\b",
        r"^\s*find\b",
        r"^\s*ls\b",
        r"^\s*pwd\b",
        r"^\s*echo\b",
        r"^\s*printf\b",
        r"^\s*wc\b",
        r"^\s*sort\b",
        r"^\s*uniq\b",
        r"^\s*diff\b",
        r"^\s*file\b",
        r"^\s*stat\b",
        r"^\s*du\b",
        r"^\s*df\b",
        r"^\s*tree\b",
        r"^\s*which\b",
        r"^\s*whereis\b",
        r"^\s*type\b",
        r"^\s*env\b",
        r"^\s*printenv\b",
        r"^\s*uname\b",
        r"^\s*whoami\b",
        r"^\s*id\b",
        r"^\s*date\b",
        r"^\s*cal\b",
        r"^\s*uptime\b",
        r"^\s*ps\b",
        r"^\s*top\b",
        r"^\s*htop\b",
        r"^\s*free\b",
        r"(?i)^\s*git\s+(status|log|diff|show|branch|remote|config\s+--get)",
        r"(?i)^\s*git\s+ls-",
        r"(?i)^\s*npm\s+(list|ls|view|info|search|outdated|audit)",
        r"(?i)^\s*yarn\s+(list|info|why|audit)",
        r"(?i)^\s*node\s+--version",
        r"(?i)^\s*python\s+--version",
        r"(?i)^\s*curl\s",
        r"(?i)^\s*wget\s+-O\s*-",
        r"^\s*jq\b",
        r"(?i)^\s*sed\s+-n",
        r"^\s*awk\b",
        r"^\s*rg\b",
        r"^\s*fd\b",
        r"^\s*bat\b",
        r"^\s*eza\b",
    ]
    .map(js_regex)
    .into()
});
pub fn is_safe_command(command: &str) -> bool {
    !DESTRUCTIVE.iter().any(|p| matches(p, command)) && SAFE.iter().any(|p| matches(p, command))
}
pub fn clean_step_text(text: &str) -> String {
    static BOLD: LazyLock<Regex> = LazyLock::new(|| js_regex(r"\*{1,2}([^*]+)\*{1,2}"));
    static CODE: LazyLock<Regex> = LazyLock::new(|| js_regex(r"`([^`]+)`"));
    static VERB: LazyLock<Regex> = LazyLock::new(|| {
        js_regex(
            r"(?i)^(Use|Run|Execute|Create|Write|Read|Check|Verify|Update|Modify|Add|Remove|Delete|Install)\s+(the\s+)?",
        )
    });
    static SPACE: LazyLock<Regex> = LazyLock::new(|| js_regex(r"\s+"));
    let cleaned = BOLD.replace_all(text, "$1");
    let cleaned = CODE.replace_all(&cleaned, "$1");
    let cleaned = VERB.replace(&cleaned, "");
    let cleaned = SPACE.replace_all(&cleaned, " ");
    let cleaned = js_trim(&cleaned);
    let mut cleaned = match cleaned.chars().next() {
        Some(first) if first.len_utf16() == 1 => format!("{}{}", first.to_uppercase(), &cleaned[first.len_utf8()..]),
        _ => cleaned.to_owned(),
    };
    if utf16_len(&cleaned) > 50 {
        cleaned = format!("{}...", utf16_slice_to_utf8(&cleaned, 0, 47));
    }
    cleaned
}
pub fn extract_todo_items(message: &str) -> Vec<TodoItem> {
    static HEADER: LazyLock<Regex> = LazyLock::new(|| js_regex(r"(?i)\*{0,2}Plan:\*{0,2}\s*\n"));
    static NUMBERED: LazyLock<Regex> = LazyLock::new(|| js_regex(r"(?m)^\s*(\d+)[.)]\s+\*{0,2}([^*\n]+)"));
    static TRAILING: LazyLock<Regex> = LazyLock::new(|| js_regex(r"\*{1,2}$"));
    let Some(header) = HEADER.find(message).expect("header regex") else {
        return vec![];
    };
    let mut items = Vec::new();
    for matched in NUMBERED.captures_iter(&message[header.end()..]) {
        let matched = matched.expect("numbered regex");
        let text = TRAILING.replace(js_trim(&matched[2]), "");
        let text = js_trim(&text);
        if utf16_len(text) <= 5 || text.starts_with(['`', '/', '-']) {
            continue;
        }
        let text = clean_step_text(text);
        if utf16_len(&text) > 3 {
            items.push(TodoItem {
                step: items.len() + 1,
                text,
                completed: false,
            });
        }
    }
    items
}
pub fn extract_done_steps(message: &str) -> Vec<f64> {
    static DONE: LazyLock<Regex> = LazyLock::new(|| js_regex(r"(?i)\[DONE:([0-9]+)\]"));
    DONE.captures_iter(message)
        .filter_map(|c| c.expect("done regex")[1].parse::<f64>().ok().filter(|n| n.is_finite()))
        .collect()
}
pub fn mark_completed_steps(text: &str, items: &mut [TodoItem]) -> usize {
    let steps = extract_done_steps(text);
    for step in &steps {
        if let Some(item) = items.iter_mut().find(|item| item.step as f64 == *step) {
            item.completed = true;
        }
    }
    steps.len()
}
