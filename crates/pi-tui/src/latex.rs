//! LaTeX-to-terminal-text renderer.
//!
//! Ported from `packages/tui/src/latex.ts`. Output is byte-identical to the
//! TypeScript renderer, including its private-use layout markers and the
//! display-mode stacking rules.

use std::sync::LazyLock;

use regex::Regex;

use crate::js_compat::{is_js_whitespace_char, js_trim, js_trim_end, js_trim_start};
use crate::utils::visible_width;

// LaTeX command tables ported verbatim from `packages/tui/src/latex.ts`.

const SYMBOLS: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ϵ"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("varkappa", "ϰ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("varpi", "ϖ"),
    ("rho", "ρ"),
    ("varrho", "ϱ"),
    ("sigma", "σ"),
    ("varsigma", "ς"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "ϕ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("pm", "±"),
    ("mp", "∓"),
    ("times", "×"),
    ("div", "÷"),
    ("cdot", "·"),
    ("ast", "∗"),
    ("star", "⋆"),
    ("circ", "∘"),
    ("bullet", "•"),
    ("oplus", "⊕"),
    ("ominus", "⊖"),
    ("otimes", "⊗"),
    ("oslash", "⊘"),
    ("odot", "⊙"),
    ("bigcirc", "○"),
    ("dagger", "†"),
    ("ddagger", "‡"),
    ("amalg", "⨿"),
    ("uplus", "⊎"),
    ("sqcap", "⊓"),
    ("sqcup", "⊔"),
    ("triangleleft", "◁"),
    ("triangleright", "▷"),
    ("wr", "≀"),
    ("cap", "∩"),
    ("cup", "∪"),
    ("bigcap", "⋂"),
    ("bigcup", "⋃"),
    ("bigwedge", "⋀"),
    ("bigvee", "⋁"),
    ("bigsqcup", "⨆"),
    ("biguplus", "⨄"),
    ("bigoplus", "⨁"),
    ("bigotimes", "⨂"),
    ("bigodot", "⨀"),
    ("setminus", "∖"),
    ("in", "∈"),
    ("notin", "∉"),
    ("ni", "∋"),
    ("subset", "⊂"),
    ("supset", "⊃"),
    ("subseteq", "⊆"),
    ("supseteq", "⊇"),
    ("sqsubset", "⊏"),
    ("sqsupset", "⊐"),
    ("sqsubseteq", "⊑"),
    ("sqsupseteq", "⊒"),
    ("prec", "≺"),
    ("preceq", "≼"),
    ("succ", "≻"),
    ("succeq", "≽"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("le", "≤"),
    ("leq", "≤"),
    ("leqslant", "≤"),
    ("ge", "≥"),
    ("geq", "≥"),
    ("geqslant", "≥"),
    ("ne", "≠"),
    ("neq", "≠"),
    ("equiv", "≡"),
    ("approx", "≈"),
    ("sim", "∼"),
    ("simeq", "≃"),
    ("cong", "≅"),
    ("asymp", "≍"),
    ("doteq", "≐"),
    ("propto", "∝"),
    ("parallel", "∥"),
    ("perp", "⊥"),
    ("mid", "∣"),
    ("vdash", "⊢"),
    ("dashv", "⊣"),
    ("models", "⊨"),
    ("Vdash", "⊩"),
    ("Vvdash", "⊪"),
    ("nvdash", "⊬"),
    ("nvDash", "⊭"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("nexists", "∄"),
    ("neg", "¬"),
    ("land", "∧"),
    ("wedge", "∧"),
    ("lor", "∨"),
    ("vee", "∨"),
    ("to", "→"),
    ("rightarrow", "→"),
    ("longrightarrow", "→"),
    ("leftarrow", "←"),
    ("longleftarrow", "←"),
    ("gets", "←"),
    ("leftrightarrow", "↔"),
    ("longleftrightarrow", "↔"),
    ("hookleftarrow", "↩"),
    ("hookrightarrow", "↪"),
    ("twoheadleftarrow", "↞"),
    ("twoheadrightarrow", "↠"),
    ("leftharpoonup", "↼"),
    ("leftharpoondown", "↽"),
    ("rightharpoonup", "⇀"),
    ("rightharpoondown", "⇁"),
    ("rightleftharpoons", "⇌"),
    ("leftrightharpoons", "⇋"),
    ("nearrow", "↗"),
    ("searrow", "↘"),
    ("swarrow", "↙"),
    ("nwarrow", "↖"),
    ("rightsquigarrow", "⇝"),
    ("leadsto", "⇝"),
    ("Rightarrow", "⇒"),
    ("Longrightarrow", "⇒"),
    ("Leftarrow", "⇐"),
    ("Longleftarrow", "⇐"),
    ("Leftrightarrow", "⇔"),
    ("Longleftrightarrow", "⇔"),
    ("implies", "⇒"),
    ("iff", "⇔"),
    ("mapsto", "↦"),
    ("longmapsto", "↦"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("int", "∫"),
    ("iint", "∬"),
    ("iiint", "∭"),
    ("oint", "∮"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("coprod", "∐"),
    ("infty", "∞"),
    ("emptyset", "∅"),
    ("varnothing", "∅"),
    ("angle", "∠"),
    ("therefore", "∴"),
    ("because", "∵"),
    ("aleph", "ℵ"),
    ("beth", "ℶ"),
    ("gimel", "ℷ"),
    ("daleth", "ℸ"),
    ("top", "⊤"),
    ("bot", "⊥"),
    ("triangle", "△"),
    ("square", "□"),
    ("lozenge", "◊"),
    ("checkmark", "✓"),
    ("complement", "∁"),
    ("wp", "℘"),
    ("prime", "′"),
    ("ldots", "…"),
    ("dots", "…"),
    ("cdots", "⋯"),
    ("vdots", "⋮"),
    ("ddots", "⋱"),
    ("ell", "ℓ"),
    ("hbar", "ℏ"),
    ("Im", "ℑ"),
    ("Re", "ℜ"),
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("vert", "|"),
    ("lvert", "|"),
    ("rvert", "|"),
    ("Vert", "‖"),
    ("lVert", "‖"),
    ("rVert", "‖"),
    ("lbrace", "{"),
    ("rbrace", "}"),
    ("backslash", "\\"),
    ("lfloor", "⌊"),
    ("rfloor", "⌋"),
    ("lceil", "⌈"),
    ("rceil", "⌉"),
    ("colon", ":"),
];

const NEGATED_SYMBOLS: &[(&str, &str)] = &[
    ("<", "≮"),
    (">", "≯"),
    ("=", "≠"),
    ("∈", "∉"),
    ("∋", "∌"),
    ("∣", "∤"),
    ("∥", "∦"),
    ("∼", "≁"),
    ("≃", "≄"),
    ("≅", "≇"),
    ("≈", "≉"),
    ("≡", "≢"),
    ("≤", "≰"),
    ("≥", "≱"),
    ("≺", "⊀"),
    ("≻", "⊁"),
    ("⊂", "⊄"),
    ("⊃", "⊅"),
    ("⊆", "⊈"),
    ("⊇", "⊉"),
    ("⊢", "⊬"),
    ("⊨", "⊭"),
    ("↔", "↮"),
    ("←", "↚"),
    ("→", "↛"),
    ("⇒", "⇏"),
    ("⇐", "⇍"),
    ("⇔", "⇎"),
    ("≼", "⋠"),
    ("≽", "⋡"),
];

const BLACKBOARD: &[(&str, &str)] = &[
    ("C", "ℂ"),
    ("H", "ℍ"),
    ("N", "ℕ"),
    ("P", "ℙ"),
    ("Q", "ℚ"),
    ("R", "ℝ"),
    ("Z", "ℤ"),
];

const SUPERSCRIPTS: &[(&str, &str)] = &[
    ("0", "⁰"),
    ("1", "¹"),
    ("2", "²"),
    ("3", "³"),
    ("4", "⁴"),
    ("5", "⁵"),
    ("6", "⁶"),
    ("7", "⁷"),
    ("8", "⁸"),
    ("9", "⁹"),
    ("+", "⁺"),
    ("-", "⁻"),
    ("=", "⁼"),
    ("(", "⁽"),
    (")", "⁾"),
    ("a", "ᵃ"),
    ("b", "ᵇ"),
    ("c", "ᶜ"),
    ("d", "ᵈ"),
    ("e", "ᵉ"),
    ("f", "ᶠ"),
    ("g", "ᵍ"),
    ("h", "ʰ"),
    ("i", "ⁱ"),
    ("j", "ʲ"),
    ("k", "ᵏ"),
    ("l", "ˡ"),
    ("m", "ᵐ"),
    ("n", "ⁿ"),
    ("o", "ᵒ"),
    ("p", "ᵖ"),
    ("r", "ʳ"),
    ("s", "ˢ"),
    ("t", "ᵗ"),
    ("u", "ᵘ"),
    ("v", "ᵛ"),
    ("w", "ʷ"),
    ("x", "ˣ"),
    ("y", "ʸ"),
    ("z", "ᶻ"),
];

const SUBSCRIPTS: &[(&str, &str)] = &[
    ("0", "₀"),
    ("1", "₁"),
    ("2", "₂"),
    ("3", "₃"),
    ("4", "₄"),
    ("5", "₅"),
    ("6", "₆"),
    ("7", "₇"),
    ("8", "₈"),
    ("9", "₉"),
    ("+", "₊"),
    ("-", "₋"),
    ("=", "₌"),
    ("(", "₍"),
    (")", "₎"),
    ("a", "ₐ"),
    ("e", "ₑ"),
    ("h", "ₕ"),
    ("i", "ᵢ"),
    ("j", "ⱼ"),
    ("k", "ₖ"),
    ("l", "ₗ"),
    ("m", "ₘ"),
    ("n", "ₙ"),
    ("o", "ₒ"),
    ("p", "ₚ"),
    ("r", "ᵣ"),
    ("s", "ₛ"),
    ("t", "ₜ"),
    ("u", "ᵤ"),
    ("v", "ᵥ"),
    ("x", "ₓ"),
];

const ACCENTS: &[(&str, &str)] = &[
    ("acute", "́"),
    ("bar", "̅"),
    ("breve", "̆"),
    ("check", "̌"),
    ("ddot", "̈"),
    ("dot", "̇"),
    ("grave", "̀"),
    ("hat", "̂"),
    ("mathring", "̊"),
    ("overleftarrow", "⃖"),
    ("overleftrightarrow", "⃡"),
    ("overline", "̅"),
    ("overrightarrow", "⃗"),
    ("tilde", "̃"),
    ("underline", "̲"),
    ("vec", "⃗"),
    ("widehat", "̂"),
    ("widetilde", "̃"),
];

const NAMED_OPERATORS: &[&str] = &[
    "arccos", "arcsin", "arctan", "arg", "cos", "cosh", "cot", "coth", "csc", "deg", "det", "dim", "exp", "gcd", "hom",
    "inf", "ker", "lg", "lim", "liminf", "limsup", "ln", "log", "max", "min", "Pr", "sec", "sin", "sinh", "sup", "tan",
    "tanh",
];

const LIMIT_OPERATORS: &[&str] = &[
    "argmax", "argmin", "inf", "injlim", "lim", "liminf", "limsup", "max", "min", "projlim", "sup",
];

const DISPLAY_LIMIT_SYMBOLS: &[&str] = &[
    "bigcap",
    "bigcup",
    "bigodot",
    "bigoplus",
    "bigotimes",
    "bigsqcup",
    "biguplus",
    "bigvee",
    "bigwedge",
    "coprod",
    "int",
    "iint",
    "iiint",
    "oint",
    "prod",
    "sum",
];

const RELATION_COMMANDS: &[&str] = &[
    "Leftarrow",
    "Leftrightarrow",
    "Longleftarrow",
    "Longleftrightarrow",
    "Longrightarrow",
    "Rightarrow",
    "Vdash",
    "Vvdash",
    "approx",
    "asymp",
    "cong",
    "dashv",
    "doteq",
    "downarrow",
    "equiv",
    "ge",
    "geq",
    "geqslant",
    "gets",
    "gg",
    "hookleftarrow",
    "hookrightarrow",
    "iff",
    "implies",
    "in",
    "leadsto",
    "le",
    "leftarrow",
    "leftharpoondown",
    "leftharpoonup",
    "leftrightarrow",
    "leftrightharpoons",
    "leq",
    "leqslant",
    "ll",
    "longleftarrow",
    "longleftrightarrow",
    "longmapsto",
    "longrightarrow",
    "mapsto",
    "mid",
    "models",
    "ne",
    "nearrow",
    "neq",
    "ni",
    "notin",
    "nvdash",
    "nvDash",
    "nwarrow",
    "parallel",
    "perp",
    "prec",
    "preceq",
    "propto",
    "rightharpoondown",
    "rightharpoonup",
    "rightleftharpoons",
    "rightarrow",
    "rightsquigarrow",
    "searrow",
    "sim",
    "simeq",
    "sqsubset",
    "sqsubseteq",
    "sqsupset",
    "sqsupseteq",
    "subset",
    "subseteq",
    "succ",
    "succeq",
    "supset",
    "supseteq",
    "swarrow",
    "to",
    "triangleleft",
    "triangleright",
    "twoheadleftarrow",
    "twoheadrightarrow",
    "uparrow",
    "vdash",
];

const SPACING_COMMANDS: &[&str] = &[
    ",",
    ":",
    ";",
    " ",
    ">",
    "enspace",
    "enskip",
    "medspace",
    "quad",
    "qquad",
    "thickspace",
    "thinspace",
];

const NEGATIVE_SPACING_COMMANDS: &[&str] = &["!", "negmedspace", "negthickspace", "negthinspace"];

const IGNORED_COMMANDS: &[&str] = &[
    "displaystyle",
    "limits",
    "nolimits",
    "scriptstyle",
    "scriptscriptstyle",
    "textstyle",
];

const SIZE_COMMANDS: &[&str] = &[
    "big", "Big", "bigg", "Bigg", "bigl", "Bigl", "biggl", "Biggl", "bigr", "Bigr", "biggr", "Biggr",
];

const PLAIN_WRAPPERS: &[&str] = &[
    "emph",
    "mathcal",
    "mathbf",
    "mathfrak",
    "mathit",
    "mathrm",
    "mathnormal",
    "mathscr",
    "mathsf",
    "mathtt",
    "mathup",
    "mbox",
    "overbrace",
    "pmb",
    "smash",
    "substack",
    "text",
    "textbf",
    "textit",
    "textmd",
    "textnormal",
    "textrm",
    "textsc",
    "textsf",
    "textsl",
    "texttt",
    "textup",
    "underbrace",
    "bm",
    "boldsymbol",
];

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(name, _)| *name == key).map(|(_, value)| *value)
}

fn in_set(set: &[&str], value: &str) -> bool {
    set.contains(&value)
}

static SCRIPT_SPACING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*([=+-])\s*").expect("script spacing pattern"));
static SIMPLE_NUMERATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}.]+$").expect("numerator pattern"));
static SIMPLE_DENOMINATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{N}.]+$").expect("denominator pattern"));
static ALIGNED_AT_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\{[^}]*\}").expect("alignedat prefix pattern"));
static ROW_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\\\(?:\[[^\]\n]*\])?").expect("row separator pattern"));
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*$").expect("trailing comma pattern"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").expect("space pattern"));
static LEFT_SPACING_CHAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[\\p{L}\\p{N}\\]}\\u{f0001}]$").expect("left spacing pattern"));
static RIGHT_SPACING_CHAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[\\p{L}\\p{N}√\\u{f0000}]$").expect("right spacing pattern"));

const NAMED_OPERATOR_START: char = '\u{f0004}';
const NAMED_OPERATOR_END: char = '\u{f0005}';
const LAYOUT_MARKER_START: char = '\u{f0000}';
const LAYOUT_MARKER_END: char = '\u{f0001}';
const PROTECTED_SPACE: char = '\u{f0002}';
const NEGATIVE_SPACE: &str = "\u{0}";

fn replace_characters(value: &str, table: &'static [(&'static str, &'static str)]) -> Option<String> {
    let mut result = String::new();
    for character in value.chars() {
        let replacement = lookup(table, &character.to_string())?;
        result.push_str(replacement);
    }
    Some(result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Sub,
    Sup,
}

fn format_script(value: &str, kind: ScriptKind) -> String {
    let value = js_trim(value);
    let replacements = match kind {
        ScriptKind::Sub => SUBSCRIPTS,
        ScriptKind::Sup => SUPERSCRIPTS,
    };
    let spaced = SCRIPT_SPACING.replace_all(value, "$1");
    if let Some(unicode) = replace_characters(&spaced, replacements) {
        return unicode;
    }

    let prefix = match kind {
        ScriptKind::Sub => "_",
        ScriptKind::Sup => "^",
    };
    let single_character = value.chars().count() == 1;
    let subscript_word = kind == ScriptKind::Sub && !value.is_empty() && value.chars().all(|c| c.is_ascii_alphabetic());
    if single_character || subscript_word {
        format!("{prefix}{value}")
    } else {
        format!("{prefix}({value})")
    }
}

fn format_fraction(numerator: &str, denominator: &str) -> String {
    let numerator = js_trim(numerator);
    let denominator = js_trim(denominator);
    let simple_numerator = SIMPLE_NUMERATOR.is_match(numerator);
    let simple_denominator = SIMPLE_DENOMINATOR.is_match(denominator) || denominator.chars().count() == 1;
    let numerator = if simple_numerator {
        numerator.to_string()
    } else {
        format!("({numerator})")
    };
    let denominator = if simple_denominator {
        denominator.to_string()
    } else {
        format!("({denominator})")
    };
    format!("{numerator}/{denominator}")
}

fn format_root(value: &str, symbol: &str) -> String {
    let value = js_trim(value);
    if SIMPLE_NUMERATOR.is_match(value) {
        format!("{symbol}{value}")
    } else {
        format!("{symbol}({value})")
    }
}

fn normalize_output(value: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for character in value.chars() {
        let spaced_out = character == NAMED_OPERATOR_START
            && previous.is_some_and(|previous| LEFT_SPACING_CHAR.is_match(&previous.to_string()));
        spaced.push(if spaced_out { ' ' } else { character });
        previous = Some(character);
    }
    let spaced = spaced.replace(NAMED_OPERATOR_START, "");

    let mut characters = spaced.chars().peekable();
    let mut right_spaced = String::new();
    while let Some(character) = characters.next() {
        let spaced_out = character == NAMED_OPERATOR_END
            && characters
                .peek()
                .is_some_and(|next| RIGHT_SPACING_CHAR.is_match(&next.to_string()));
        right_spaced.push(if spaced_out { ' ' } else { character });
    }
    let right_spaced = right_spaced.replace(NAMED_OPERATOR_END, "");

    let lines: Vec<&str> = right_spaced.split('\n').collect();
    let mut kept: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let collapsed = SPACES.replace_all(line, " ");
        let trimmed = js_trim(&collapsed);
        if !trimmed.is_empty() || (index > 0 && index < lines.len() - 1) {
            kept.push(trimmed.to_string());
        }
    }
    js_trim(&kept.join("\n")).to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LayoutNode {
    Fraction {
        numerator: String,
        denominator: String,
    },
    Operator {
        operator: String,
        lower: Option<String>,
        upper: Option<String>,
    },
    Matrix {
        lines: Vec<String>,
        baseline: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Layout {
    lines: Vec<String>,
    width: usize,
    baseline: usize,
}

fn pad_layout_line(line: &str, width: usize, centered: bool) -> String {
    let padding = width.saturating_sub(visible_width(line));
    let left = if centered { padding / 2 } else { 0 };
    format!("{}{}{}", " ".repeat(left), line, " ".repeat(padding - left))
}

fn join_layouts(layouts: &[Layout]) -> Layout {
    if layouts.is_empty() {
        return Layout {
            lines: vec![String::new()],
            width: 0,
            baseline: 0,
        };
    }

    let baseline = layouts.iter().map(|layout| layout.baseline).max().unwrap_or(0);
    let below = layouts
        .iter()
        .map(|layout| layout.lines.len().saturating_sub(layout.baseline + 1))
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for row in 0..=baseline + below {
        let mut line = String::new();
        for layout in layouts {
            let source_row = row as isize - baseline as isize + layout.baseline as isize;
            if source_row >= 0 && (source_row as usize) < layout.lines.len() {
                line.push_str(&pad_layout_line(
                    &layout.lines[source_row as usize],
                    layout.width,
                    false,
                ));
            } else {
                line.push_str(&" ".repeat(layout.width));
            }
        }
        lines.push(js_trim_end(&line).to_string());
    }

    Layout {
        lines,
        width: layouts.iter().map(|layout| layout.width).sum(),
        baseline,
    }
}

fn find_layout_marker(line: &str, from: usize) -> Option<(usize, usize, usize)> {
    let start = line[from..].find(LAYOUT_MARKER_START)? + from;
    let digits_start = start + LAYOUT_MARKER_START.len_utf8();
    let digits_end = line[digits_start..].find(LAYOUT_MARKER_END)? + digits_start;
    let index = line[digits_start..digits_end].parse().ok()?;
    Some((start, index, digits_end + LAYOUT_MARKER_END.len_utf8()))
}

fn trailing_layout_index(value: &str) -> Option<usize> {
    if !value.ends_with(LAYOUT_MARKER_END) {
        return None;
    }
    let start = value.rfind(LAYOUT_MARKER_START)?;
    let digits_start = start + LAYOUT_MARKER_START.len_utf8();
    let digits_end = value.len() - LAYOUT_MARKER_END.len_utf8();
    if digits_start > digits_end {
        return None;
    }
    value[digits_start..digits_end].parse().ok()
}

fn render_layout(source: &str, nodes: &[LayoutNode]) -> Layout {
    let mut rendered_lines: Vec<String> = Vec::new();
    let mut first_baseline = 0;

    for source_line in source.split('\n') {
        let mut layouts: Vec<Layout> = Vec::new();
        let mut position = 0;
        let mut previous_node: Option<&LayoutNode> = None;
        let mut scan = 0;

        while let Some((start, index, end)) = find_layout_marker(source_line, scan) {
            scan = end;
            let Some(node) = nodes.get(index) else {
                continue;
            };
            if start > position {
                let sliced = &source_line[position..start];
                let trimmed = if previous_node.is_some() {
                    js_trim_start(sliced)
                } else {
                    sliced
                };
                let trimmed = js_trim_end(trimmed);
                let preserve_leading_space = matches!(previous_node, Some(LayoutNode::Matrix { .. }))
                    && sliced.starts_with(is_js_whitespace_char);
                let preserve_trailing_space =
                    matches!(node, LayoutNode::Matrix { .. }) && sliced.ends_with(is_js_whitespace_char);
                let text = if !trimmed.is_empty() {
                    format!(
                        "{}{}{}",
                        if preserve_leading_space { " " } else { "" },
                        trimmed,
                        if preserve_trailing_space { " " } else { "" }
                    )
                } else if preserve_leading_space || preserve_trailing_space {
                    " ".to_string()
                } else {
                    String::new()
                };
                layouts.push(Layout {
                    width: visible_width(&text),
                    lines: vec![text],
                    baseline: 0,
                });
            }

            layouts.push(render_node_layout(node, nodes));
            position = end;
            previous_node = Some(node);
        }

        if position < source_line.len() {
            let sliced = &source_line[position..];
            let trimmed = if previous_node.is_some() {
                js_trim_start(sliced)
            } else {
                sliced
            };
            let text = if matches!(previous_node, Some(LayoutNode::Matrix { .. }))
                && sliced.starts_with(is_js_whitespace_char)
            {
                format!(" {trimmed}")
            } else {
                trimmed.to_string()
            };
            layouts.push(Layout {
                width: visible_width(&text),
                lines: vec![text],
                baseline: 0,
            });
        }

        let line_layout = join_layouts(&layouts);
        if rendered_lines.is_empty() {
            first_baseline = line_layout.baseline;
        }
        rendered_lines.extend(line_layout.lines);
    }

    Layout {
        width: rendered_lines.iter().map(|line| visible_width(line)).max().unwrap_or(0),
        lines: rendered_lines,
        baseline: first_baseline,
    }
}

fn render_node_layout(node: &LayoutNode, nodes: &[LayoutNode]) -> Layout {
    match node {
        LayoutNode::Fraction { numerator, denominator } => {
            let numerator = render_layout(numerator, nodes);
            let denominator = render_layout(denominator, nodes);
            let content_width = numerator.width.max(denominator.width).max(1);
            let width = content_width + 2;
            let mut lines: Vec<String> = numerator
                .lines
                .iter()
                .map(|line| pad_layout_line(line, width, true))
                .collect();
            lines.push(format!(" {} ", "─".repeat(content_width)));
            lines.extend(denominator.lines.iter().map(|line| pad_layout_line(line, width, true)));
            Layout {
                lines,
                width,
                baseline: numerator.lines.len(),
            }
        }
        LayoutNode::Operator { operator, lower, upper } => {
            let content_width = visible_width(operator)
                .max(lower.as_deref().map_or(0, visible_width))
                .max(upper.as_deref().map_or(0, visible_width));
            let mut lines = Vec::new();
            if let Some(upper) = upper {
                lines.push(format!("{} ", pad_layout_line(upper, content_width, true)));
            }
            lines.push(format!("{} ", pad_layout_line(operator, content_width, true)));
            if let Some(lower) = lower {
                lines.push(format!("{} ", pad_layout_line(lower, content_width, true)));
            }
            Layout {
                lines,
                width: content_width + 1,
                baseline: if upper.is_some() { 1 } else { 0 },
            }
        }
        LayoutNode::Matrix { lines, baseline } => {
            let width = lines.iter().map(|line| visible_width(line)).max().unwrap_or(0);
            Layout {
                lines: lines.iter().map(|line| pad_layout_line(line, width, false)).collect(),
                width,
                baseline: *baseline,
            }
        }
    }
}

struct LatexParser<'src, 'nodes> {
    source: &'src str,
    layout_nodes: &'nodes mut Vec<LayoutNode>,
    display: bool,
    position: usize,
    supported: bool,
    stack_fractions: bool,
}

impl<'src, 'nodes> LatexParser<'src, 'nodes> {
    fn new(source: &'src str, layout_nodes: &'nodes mut Vec<LayoutNode>, display: bool) -> Self {
        Self {
            source,
            layout_nodes,
            display,
            position: 0,
            supported: true,
            stack_fractions: true,
        }
    }

    fn render(&mut self) -> Option<String> {
        let rendered = self.parse_sequence(None);
        if !self.supported || self.position != self.source.len() {
            return None;
        }
        Some(normalize_output(&rendered))
    }

    fn current(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let character = self.current()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn parse_sequence(&mut self, end_character: Option<char>) -> String {
        let mut result = String::new();
        while let Some(character) = self.current() {
            if Some(character) == end_character {
                self.advance_char();
                return result;
            }
            if character == '}' {
                self.supported = false;
                return result;
            }
            if character == '{' {
                self.advance_char();
                result.push_str(&self.parse_sequence(Some('}')));
                continue;
            }
            if character == '\\' {
                let command = self.parse_command();
                if command == NEGATIVE_SPACE {
                    result = js_trim_end(&result).to_string();
                    if result.ends_with(NAMED_OPERATOR_END) {
                        result.truncate(result.len() - NAMED_OPERATOR_END.len_utf8());
                    }
                } else {
                    result.push_str(&command);
                }
                continue;
            }
            if character == '^' || character == '_' {
                self.advance_char();
                result = js_trim_end(&result).to_string();
                let script = format_script(
                    &self.parse_required_argument(false),
                    if character == '_' {
                        ScriptKind::Sub
                    } else {
                        ScriptKind::Sup
                    },
                );
                if result.ends_with(NAMED_OPERATOR_END) {
                    result.truncate(result.len() - NAMED_OPERATOR_END.len_utf8());
                    result.push_str(&script);
                    result.push(NAMED_OPERATOR_END);
                } else {
                    result.push_str(&script);
                }
                continue;
            }
            if is_js_whitespace_char(character) {
                result.push_str(&self.parse_whitespace());
                continue;
            }
            if character == '=' || character == '<' || character == '>' {
                result = format!("{} {character} ", js_trim_end(&result));
                self.advance_char();
                continue;
            }
            if character == '&' {
                self.advance_char();
                continue;
            }
            if character == '~' {
                self.advance_char();
                result.push(' ');
                continue;
            }
            if character == '.'
                && let Some(index) = trailing_layout_index(&result)
                && let Some(LayoutNode::Matrix { lines, .. }) = self.layout_nodes.get_mut(index)
                && let Some(last_line) = lines.last_mut()
            {
                last_line.push('.');
                self.advance_char();
                continue;
            }
            result.push(character);
            self.advance_char();
        }

        if end_character.is_some() {
            self.supported = false;
        }
        result
    }

    fn parse_whitespace(&mut self) -> String {
        while let Some(character) = self.current() {
            if !is_js_whitespace_char(character) {
                break;
            }
            self.advance_char();
        }
        " ".to_string()
    }

    fn parse_command(&mut self) -> String {
        self.advance_char();
        let Some(first) = self.current() else {
            self.supported = false;
            return String::new();
        };

        let command = if first.is_ascii_alphabetic() {
            let start = self.position;
            while self.current().is_some_and(|character| character.is_ascii_alphabetic()) {
                self.advance_char();
            }
            self.source[start..self.position].to_string()
        } else {
            self.advance_char();
            first.to_string()
        };

        self.dispatch_command(&command)
    }

    fn dispatch_command(&mut self, command: &str) -> String {
        if command == "\\" {
            return "\n".to_string();
        }
        if in_set(SPACING_COMMANDS, command) {
            return " ".to_string();
        }
        if in_set(NEGATIVE_SPACING_COMMANDS, command) {
            return NEGATIVE_SPACE.to_string();
        }
        if in_set(IGNORED_COMMANDS, command) {
            return String::new();
        }
        if matches!(command, "{" | "}" | "$" | "%" | "#" | "_" | "&") {
            return command.to_string();
        }
        if command == "|" {
            return "‖".to_string();
        }
        if command == "not" {
            return self.parse_negation();
        }
        if in_set(LIMIT_OPERATORS, command) {
            return self.parse_operator(command, LowerStyle::Bracket, true, true);
        }
        if let Some(symbol) = lookup(SYMBOLS, command) {
            if in_set(DISPLAY_LIMIT_SYMBOLS, command) {
                return self.parse_operator(symbol, LowerStyle::Script, true, false);
            }
            let spaced = command == "cdot" || command == "times" || in_set(RELATION_COMMANDS, command);
            return if spaced {
                format!(" {symbol} ")
            } else {
                symbol.to_string()
            };
        }
        if in_set(NAMED_OPERATORS, command) {
            return format!("{NAMED_OPERATOR_START}{command}{NAMED_OPERATOR_END}");
        }
        if in_set(SIZE_COMMANDS, command) {
            return String::new();
        }
        if matches!(command, "left" | "middle" | "right") {
            if self.current() == Some('.') {
                self.advance_char();
            }
            return String::new();
        }
        if matches!(command, "frac" | "dfrac" | "tfrac") {
            return self.parse_fraction_command(command);
        }
        if command == "sqrt" {
            return self.parse_root();
        }
        if matches!(command, "boxed" | "fbox") {
            return format!("[{}]", js_trim(&self.parse_required_argument(true)));
        }
        if matches!(command, "binom" | "dbinom" | "tbinom") {
            let first = self.parse_required_argument(true);
            let second = self.parse_required_argument(true);
            return format!("({first} choose {second})");
        }
        if let Some(accent) = lookup(ACCENTS, command) {
            let value = self.parse_required_argument(true);
            return if value.chars().count() == 1 {
                format!("{value}{accent}")
            } else {
                format!("{command}({value})")
            };
        }
        if command == "mathbb" {
            let value = self.parse_required_argument(true);
            let mut mapped = String::new();
            for character in value.chars() {
                match lookup(BLACKBOARD, &character.to_string()) {
                    Some(replacement) => mapped.push_str(replacement),
                    None => mapped.push(character),
                }
            }
            return mapped;
        }
        if command == "operatorname" {
            return self.parse_operatorname();
        }
        if matches!(command, "mod" | "bmod") {
            return " mod ".to_string();
        }
        if matches!(command, "pmod" | "pod") {
            let value = js_trim(&self.parse_required_argument(true)).to_string();
            return if command == "pmod" {
                format!(" (mod {value})")
            } else {
                format!(" ({value})")
            };
        }
        if matches!(command, "overset" | "stackrel") {
            let upper = self.parse_required_argument(true);
            let value = js_trim(&self.parse_required_argument(true)).to_string();
            return format!("{value}{}", format_script(&upper, ScriptKind::Sup));
        }
        if command == "underset" {
            let lower = self.parse_required_argument(true);
            let value = js_trim(&self.parse_required_argument(true)).to_string();
            return format!("{value}{}", format_script(&lower, ScriptKind::Sub));
        }
        if in_set(PLAIN_WRAPPERS, command) {
            let value = self.parse_required_argument(true);
            return if command.starts_with("text") || command == "mbox" {
                value
            } else {
                js_trim(&value).to_string()
            };
        }
        if command == "begin" {
            return self.parse_environment();
        }
        if command == "end" {
            self.supported = false;
            return String::new();
        }

        self.supported = false;
        format!("\\{command}")
    }

    fn parse_negation(&mut self) -> String {
        let value = js_trim(&self.parse_required_argument(false)).to_string();
        if let Some(negated) = lookup(NEGATED_SYMBOLS, &value) {
            return format!(" {negated} ");
        }
        let characters: Vec<char> = value.chars().collect();
        let Some(first) = characters.first() else {
            self.supported = false;
            return String::new();
        };
        let rest: String = characters[1..].iter().collect();
        format!(" {first}\u{338}{rest} ")
    }

    fn parse_fraction_command(&mut self, command: &str) -> String {
        let should_stack = self.display && self.stack_fractions && command != "tfrac";
        let numerator = self.parse_required_argument(!should_stack);
        let denominator = self.parse_required_argument(!should_stack);
        if should_stack {
            let index = self.layout_nodes.len();
            self.layout_nodes.push(LayoutNode::Fraction {
                numerator: normalize_output(&numerator),
                denominator: normalize_output(&denominator),
            });
            return format!("{LAYOUT_MARKER_START}{index}{LAYOUT_MARKER_END}");
        }
        format_fraction(&numerator, &denominator)
    }

    fn parse_root(&mut self) -> String {
        let degree = self
            .parse_optional_argument()
            .map(|degree| js_trim(&degree).to_string());
        let value = self.parse_required_argument(true);
        match degree.as_deref() {
            None | Some("2") => format_root(&value, "√"),
            Some("3") => format_root(&value, "∛"),
            Some("4") => format_root(&value, "∜"),
            Some(degree) => format!("{}{}", format_script(degree, ScriptKind::Sup), format_root(&value, "√")),
        }
    }

    fn parse_operatorname(&mut self) -> String {
        let starred = self.current() == Some('*');
        if starred {
            self.advance_char();
        }
        let operator = js_trim(&normalize_output(&self.parse_required_argument(true))).to_string();
        self.parse_operator(&operator, LowerStyle::Bracket, starred, true)
    }

    fn parse_operator(
        &mut self,
        operator: &str,
        inline_lower_style: LowerStyle,
        display_limits: bool,
        spaced: bool,
    ) -> String {
        let mut use_display_limits = display_limits;
        let mut modifier_position = self.position;
        while self
            .source
            .get(modifier_position..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|character| character == ' ' || character == '\t')
        {
            modifier_position += 1;
        }
        if let Some((limits, length)) = match_modifier(&self.source[modifier_position..]) {
            use_display_limits = limits;
            self.position = modifier_position + length;
        }

        let mut lower: Option<String> = None;
        let mut upper: Option<String> = None;
        loop {
            let mut script_position = self.position;
            while self
                .source
                .get(script_position..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|character| character == ' ' || character == '\t')
            {
                script_position += 1;
            }
            let kind = self.source.get(script_position..).and_then(|rest| rest.chars().next());
            if kind != Some('_') && kind != Some('^') {
                break;
            }
            self.position = script_position + 1;
            let value = normalize_output(&self.parse_required_argument(false)).replace(' ', "");
            if kind == Some('_') {
                if lower.is_some() {
                    self.supported = false;
                }
                lower = Some(value);
            } else {
                if upper.is_some() {
                    self.supported = false;
                }
                upper = Some(value);
            }
        }

        if self.display && use_display_limits && (lower.is_some() || upper.is_some()) {
            let index = self.layout_nodes.len();
            self.layout_nodes.push(LayoutNode::Operator {
                operator: operator.to_string(),
                lower,
                upper,
            });
            return format!("{LAYOUT_MARKER_START}{index}{LAYOUT_MARKER_END}");
        }

        let mut rendered = operator.to_string();
        if let Some(lower) = &lower {
            rendered.push_str(&match inline_lower_style {
                LowerStyle::Bracket => format!("[{lower}]"),
                LowerStyle::Script => format_script(lower, ScriptKind::Sub),
            });
        }
        if let Some(upper) = &upper {
            rendered.push_str(&format_script(upper, ScriptKind::Sup));
        }
        if spaced { format!(" {rendered} ") } else { rendered }
    }

    fn parse_required_argument(&mut self, stack_fractions: bool) -> String {
        let previous_stack_fractions = self.stack_fractions;
        self.stack_fractions = previous_stack_fractions && stack_fractions;
        let value = self.parse_required_argument_value();
        self.stack_fractions = previous_stack_fractions;
        value
    }

    fn parse_required_argument_value(&mut self) -> String {
        while let Some(character) = self.current() {
            if !is_js_whitespace_char(character) {
                break;
            }
            self.advance_char();
        }
        let Some(character) = self.current() else {
            self.supported = false;
            return String::new();
        };
        if character == '{' {
            self.advance_char();
            return self.parse_sequence(Some('}'));
        }
        if character == '\\' {
            return self.parse_command();
        }
        self.advance_char();
        character.to_string()
    }

    fn parse_optional_argument(&mut self) -> Option<String> {
        while let Some(character) = self.current() {
            if character != ' ' && character != '\t' {
                break;
            }
            self.advance_char();
        }
        if self.current() != Some('[') {
            return None;
        }
        let Some(relative_end) = self.source[self.position + 1..].find(']') else {
            self.supported = false;
            return None;
        };
        let end = self.position + 1 + relative_end;
        let value = self.source[self.position + 1..end].to_string();
        self.position = end + 1;
        Some(self.render_nested(&value, true))
    }

    fn read_raw_group(&mut self) -> Option<String> {
        while let Some(character) = self.current() {
            if character != ' ' && character != '\t' {
                break;
            }
            self.advance_char();
        }
        if self.current() != Some('{') {
            self.supported = false;
            return None;
        }

        self.advance_char();
        let start = self.position;
        let mut depth = 1;
        while let Some(character) = self.current() {
            if character == '\\' {
                self.advance_char();
                self.advance_char();
                continue;
            }
            if character == '{' {
                depth += 1;
            }
            if character == '}' {
                depth -= 1;
            }
            if depth == 0 {
                let value = self.source[start..self.position].to_string();
                self.advance_char();
                return Some(value);
            }
            self.advance_char();
        }

        self.supported = false;
        None
    }

    fn parse_environment(&mut self) -> String {
        let Some(environment) = self.read_raw_group() else {
            return String::new();
        };
        if environment.is_empty() {
            return String::new();
        }

        let end_marker = format!("\\end{{{environment}}}");
        let Some(relative_end) = self.source[self.position..].find(&end_marker) else {
            self.supported = false;
            return String::new();
        };
        let end = self.position + relative_end;
        let body = self.source[self.position..end].to_string();
        self.position = end + end_marker.len();

        if matches!(environment.as_str(), "equation" | "equation*" | "displaymath") {
            return js_trim(&self.render_nested(&body, true)).to_string();
        }

        if matches!(
            environment.as_str(),
            "aligned"
                | "align"
                | "align*"
                | "alignedat"
                | "alignat"
                | "alignat*"
                | "gather"
                | "gathered"
                | "multline"
                | "multline*"
                | "split"
        ) {
            return self.parse_aligned_environment(&environment, &body);
        }

        if matches!(environment.as_str(), "cases" | "cases*") {
            return self.parse_cases_environment(&body);
        }

        if matches!(
            environment.as_str(),
            "array" | "matrix" | "smallmatrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix"
        ) {
            let matrix_body = if environment == "array" {
                ALIGNED_AT_PREFIX.replace(&body, "").to_string()
            } else {
                body
            };
            return self.render_matrix(&environment, &matrix_body);
        }

        self.supported = false;
        body
    }

    fn parse_aligned_environment(&mut self, environment: &str, body: &str) -> String {
        let aligned_at = matches!(environment, "alignedat" | "alignat" | "alignat*");
        let aligned_body = if aligned_at {
            ALIGNED_AT_PREFIX.replace(body, "").to_string()
        } else {
            body.to_string()
        };

        let mut rows = Vec::new();
        for row in ROW_SEPARATOR.split(&aligned_body) {
            let cells: Vec<&str> = row.split('&').collect();
            let source = if aligned_at {
                let mut chunks = Vec::new();
                let mut index = 0;
                while index < cells.len() {
                    let end = (index + 2).min(cells.len());
                    chunks.push(cells[index..end].join(""));
                    index += 2;
                }
                chunks.join(" ")
            } else {
                cells.join("")
            };
            let rendered = js_trim(&self.render_nested(&source, true)).to_string();
            if !rendered.is_empty() {
                rows.push(rendered);
            }
        }
        rows.join("\n")
    }

    fn parse_cases_environment(&mut self, body: &str) -> String {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in ROW_SEPARATOR.split(body) {
            let mut cells: Vec<String> = Vec::new();
            for cell in row.split('&') {
                let rendered = self.render_nested(cell, false);
                cells.push(js_trim(&rendered).to_string());
            }
            if cells.iter().any(|cell| !cell.is_empty()) {
                rows.push(cells);
            }
        }

        let row_count = rows.len();
        let mut lines = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let value = row
                .first()
                .map(|value| TRAILING_COMMA.replace(value, "").to_string())
                .unwrap_or_default();
            let condition = row.get(1).cloned().unwrap_or_default();
            let delimiter = if index == 0 {
                "⎧"
            } else if index == row_count - 1 {
                "⎩"
            } else {
                "⎨"
            };
            let condition_prefix = if has_case_prefix(&condition) { " " } else { " if " };
            let rendered_condition = if condition.is_empty() {
                String::new()
            } else {
                format!("{condition_prefix}{condition}")
            };
            lines.push(format!("{delimiter} {value}{rendered_condition}"));
        }
        lines.join("\n")
    }

    fn render_matrix(&mut self, environment: &str, body: &str) -> String {
        let mut matrix: Vec<Vec<String>> = Vec::new();
        for row in ROW_SEPARATOR.split(body) {
            let mut cells: Vec<String> = Vec::new();
            for cell in row.split('&') {
                let rendered = self.render_nested(cell, false);
                cells.push(js_trim(&rendered).to_string());
            }
            if cells.iter().any(|cell| !cell.is_empty()) {
                matrix.push(cells);
            }
        }

        let column_count = matrix.iter().map(|row| row.len()).max().unwrap_or(0);
        let column_widths: Vec<usize> = (0..column_count)
            .map(|column| {
                matrix
                    .iter()
                    .map(|row| visible_width(row.get(column).map_or("", String::as_str)))
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let rows: Vec<String> = matrix
            .iter()
            .map(|row| {
                (0..column_count)
                    .map(|column| {
                        let cell = row.get(column).map_or("", String::as_str);
                        let padding = column_widths[column].saturating_sub(visible_width(cell));
                        format!("{cell}{}", PROTECTED_SPACE.to_string().repeat(padding))
                    })
                    .collect::<Vec<String>>()
                    .join(" │ ")
            })
            .collect();

        let lines = if matches!(environment, "array" | "matrix" | "smallmatrix") {
            rows
        } else {
            let Some(delimiter) = matrix_delimiters(environment) else {
                self.supported = false;
                return rows.join("\n");
            };
            let row_count = rows.len();
            rows.iter()
                .enumerate()
                .map(|(index, row)| {
                    let left = if index == 0 {
                        delimiter.0
                    } else if index == row_count - 1 {
                        delimiter.4
                    } else {
                        delimiter.2
                    };
                    let right = if index == 0 {
                        delimiter.1
                    } else if index == row_count - 1 {
                        delimiter.5
                    } else {
                        delimiter.3
                    };
                    format!("{left} {row} {right}")
                })
                .collect()
        };

        if lines.len() <= 1 {
            return lines.first().cloned().unwrap_or_default();
        }
        let index = self.layout_nodes.len();
        self.layout_nodes.push(LayoutNode::Matrix { lines, baseline: 0 });
        format!("{LAYOUT_MARKER_START}{index}{LAYOUT_MARKER_END}")
    }

    fn render_nested(&mut self, source: &str, stack_fractions: bool) -> String {
        let mut nested = LatexParser::new(source, &mut *self.layout_nodes, self.display && stack_fractions);
        let rendered = nested.render();
        if rendered.is_none() {
            self.supported = false;
        }
        rendered.unwrap_or_else(|| source.to_string())
    }
}

#[derive(Clone, Copy)]
enum LowerStyle {
    Bracket,
    Script,
}

fn match_modifier(rest: &str) -> Option<(bool, usize)> {
    for (name, limits) in [("\\limits", true), ("\\nolimits", false)] {
        if let Some(after) = rest.strip_prefix(name)
            && !after.starts_with(|character: char| character.is_ascii_alphabetic())
        {
            return Some((limits, name.len()));
        }
    }
    None
}

fn has_case_prefix(condition: &str) -> bool {
    let bytes = condition.as_bytes();
    for keyword in ["if", "when", "for", "otherwise"] {
        if bytes.len() < keyword.len() || !bytes[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes()) {
            continue;
        }
        return match bytes.get(keyword.len()) {
            None => true,
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
        };
    }
    false
}

fn matrix_delimiters(
    environment: &str,
) -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    match environment {
        "pmatrix" => Some(("⎛", "⎞", "⎜", "⎟", "⎝", "⎠")),
        "bmatrix" => Some(("⎡", "⎤", "⎢", "⎥", "⎣", "⎦")),
        "Bmatrix" => Some(("⎧", "⎫", "⎨", "⎬", "⎩", "⎭")),
        "vmatrix" => Some(("│", "│", "│", "│", "│", "│")),
        "Vmatrix" => Some(("║", "║", "║", "║", "║", "║")),
        _ => None,
    }
}

/// Options for [`render_latex`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderLatexOptions {
    /// Stack fractions and operator limits vertically for display math.
    pub display: bool,
}

/// Render a basic LaTeX math expression as terminal-friendly Unicode text.
///
/// Returns `None` when the expression contains unsupported or malformed syntax.
pub fn render_latex(source: &str, options: RenderLatexOptions) -> Option<String> {
    let mut layout_nodes: Vec<LayoutNode> = Vec::new();
    let rendered = LatexParser::new(source, &mut layout_nodes, options.display).render()?;
    if layout_nodes.is_empty() {
        return Some(rendered.replace(PROTECTED_SPACE, " "));
    }

    let lines = render_layout(&rendered, &layout_nodes).lines;
    let indentation = lines
        .iter()
        .filter(|line| !js_trim(line).is_empty())
        .map(|line| line.len() - js_trim_start(line).len())
        .min()
        .unwrap_or(usize::MAX);

    let output: Vec<String> = lines
        .iter()
        .map(|line| js_trim_end(line.get(indentation..).unwrap_or("")).to_string())
        .collect();
    Some(
        js_trim_end(&output.join("\n"))
            .to_string()
            .replace(PROTECTED_SPACE, " "),
    )
}
