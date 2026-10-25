//! Port of the highlight.js 10.7.3 core engine that the coding agent uses.
//!
//! Grammars are generated from the pinned package into `grammars.json` by
//! `oracle/hljs-grammar-gen.mjs`; this module compiles and interprets them.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::JsRegex;

type GrammarRegexCache = Mutex<BTreeMap<(JsString, bool), Arc<JsRegex>>>;
type CompiledKeywords = BTreeMap<JsString, (String, i64)>;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::escape_html;

const CLASS_PREFIX: &str = "hljs-";
const COMMON_KEYWORDS: [&str; 12] = [
    "of", "and", "for", "in", "not", "or", "if", "then", "parent", "list", "value", "do",
];
const DEFAULT_KEYWORD_CLASS: &str = "keyword";
const GRAMMARS: &str = include_str!("grammars.json");

/// Result of highlighting one snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HighlightResult {
    pub value: JsString,
    pub relevance: i64,
    pub language: String,
    pub illegal: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrammarFile {
    languages: Vec<LanguageEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageEntry {
    key: String,
    root: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    aliases: Vec<String>,
    #[serde(default)]
    superset_of: Option<String>,
    #[serde(default)]
    disable_autodetect: bool,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default, deserialize_with = "null_as_default")]
    class_name_aliases: HashMap<String, String>,
    defs: HashMap<u32, ModeDef>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ModeDef {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    class_name: Option<String>,
    #[serde(default)]
    begin: Option<Pattern>,
    #[serde(default)]
    end: Option<Pattern>,
    #[serde(default)]
    illegal: Option<Illegal>,
    #[serde(default)]
    keywords: Option<Value>,
    #[serde(default, deserialize_with = "null_as_default")]
    contains: Vec<ContainsEntry>,
    #[serde(default)]
    relevance: Option<i64>,
    #[serde(default)]
    exclude_begin: bool,
    #[serde(default)]
    exclude_end: bool,
    #[serde(default)]
    return_begin: bool,
    #[serde(default)]
    return_end: bool,
    #[serde(default)]
    ends_with_parent: bool,
    #[serde(default)]
    ends_parent: bool,
    #[serde(default)]
    starts: Option<ContainsEntry>,
    #[serde(default, deserialize_with = "null_as_default")]
    variants: Vec<ContainsEntry>,
    #[serde(default)]
    lexemes: Option<Pattern>,
    #[serde(default)]
    sub_language: Option<SubLanguage>,
    #[serde(default)]
    skip: bool,
    #[serde(default)]
    begin_keywords: Option<String>,
    #[serde(default)]
    end_same_as_begin: bool,
    #[serde(default, rename = "match")]
    match_: Option<Pattern>,
    #[serde(default, rename = "on:begin")]
    on_begin: Option<Callback>,
    #[serde(default, rename = "on:end")]
    on_end: Option<Callback>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Pattern {
    Literal(String),
    Regex(RegexSource),
}

#[derive(Debug, Clone, Deserialize)]
struct RegexSource {
    #[serde(rename = "$regex")]
    source: String,
}

impl Pattern {
    fn source(&self) -> &str {
        match self {
            Self::Literal(source) => source,
            Self::Regex(regex) => &regex.source,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Illegal {
    One(Pattern),
    Many(Vec<Pattern>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ContainsEntry {
    SelfRef(String),
    Mode(ModeRef),
    Group(Vec<ContainsEntry>),
}

#[derive(Debug, Clone, Deserialize)]
struct ModeRef {
    #[serde(rename = "$ref")]
    id: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SubLanguage {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
struct Callback {
    #[serde(rename = "$fn")]
    name: String,
}

struct CompiledLanguage {
    superset_of: Option<String>,
    disable_autodetect: bool,
    case_insensitive: bool,
    class_name_aliases: HashMap<String, String>,
    modes: Vec<CompiledMode>,
    root: usize,
}

#[derive(Default)]
struct CompiledMode {
    class_name: Option<String>,
    first_parent: Option<usize>,
    begin: Option<Arc<JsRegex>>,
    begin_source: JsString,
    end: Option<Arc<JsRegex>>,
    end_source: JsString,
    illegal: Option<Arc<JsRegex>>,
    illegal_source: JsString,
    terminator_end: JsString,
    keywords: CompiledKeywords,
    keyword_pattern: Option<Arc<JsRegex>>,
    contains: Vec<usize>,
    relevance: i64,
    exclude_begin: bool,
    exclude_end: bool,
    return_begin: bool,
    return_end: bool,
    ends_with_parent: bool,
    ends_parent: bool,
    skip: bool,
    sub_language: Option<SubLanguage>,
    on_begin: Option<String>,
    on_end: Option<String>,
    end_same_as_begin: bool,
    starts: Option<usize>,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    Begin,
    End,
    Illegal,
}

#[derive(Clone)]
struct Rule {
    kind: RuleKind,
    mode: Option<usize>,
    pattern: JsString,
}

struct MatchInfo {
    index: usize,
    text: JsString,
    groups: Vec<Option<JsString>>,
    kind: RuleKind,
    mode: Option<usize>,
    position: usize,
}

/// Treats JSON `null` as the default value, matching falsy JavaScript fields.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn grammar_file() -> &'static GrammarFile {
    static FILE: LazyLock<GrammarFile> =
        LazyLock::new(|| serde_json::from_str(GRAMMARS).expect("generated grammars must parse"));
    &FILE
}

fn compiled(key: &str) -> Option<Arc<CompiledLanguage>> {
    static CACHE: LazyLock<Mutex<HashMap<String, Option<Arc<CompiledLanguage>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = cache.get(key) {
        return entry.clone();
    }
    let entry = grammar_file()
        .languages
        .iter()
        .find(|language| language.key == key)
        .map(|language| Arc::new(compile_language(language.clone())));
    cache.insert(key.to_owned(), entry.clone());
    entry
}

/// Resolves a language key or alias to the canonical registered key.
///
/// Mirrors highlight.js `getLanguage(name)`: lowercase key, then alias.
pub(crate) fn resolve_language(name: &JsString) -> Option<String> {
    let lower = name.to_lowercase();
    grammar_file()
        .languages
        .iter()
        .find(|language| lower == language.key || language.aliases.iter().any(|alias| lower == *alias))
        .map(|language| language.key.clone())
}

/// True when highlight.js would find the language.
pub(crate) fn supports_language(name: &JsString) -> bool {
    resolve_language(name).is_some()
}

/// Highlights `code` as `language`, mirroring `hljs.highlight(code, {language, ignoreIllegals})`.
pub(crate) fn highlight(language: &JsString, code: &JsString, ignore_illegals: bool) -> Option<HighlightResult> {
    let key = resolve_language(language)?;
    let compiled = compiled(&key)?;
    let mut continuations = HashMap::new();
    let result = highlight_compiled(&compiled, code, ignore_illegals, None, &mut continuations);
    Some(HighlightResult {
        value: result.value,
        relevance: result.relevance,
        language: key,
        illegal: result.illegal,
    })
}

/// Mirrors `hljs.highlightAuto(code, languageSubset)`.
pub(crate) fn highlight_auto(code: &JsString, subset: Option<&[JsString]>) -> HighlightResult {
    highlight_auto_internal(code, subset).0
}

fn highlight_auto_internal(code: &JsString, subset: Option<&[JsString]>) -> (HighlightResult, Emitter) {
    let candidates: Vec<JsString> = match subset {
        Some(subset) => subset.to_vec(),
        None => grammar_file()
            .languages
            .iter()
            .map(|language| JsString::from(&language.key))
            .collect(),
    };
    let mut plaintext_emitter = Emitter::new();
    plaintext_emitter.add_text(code);
    let mut results = vec![(
        HighlightResult {
            value: escape_html(code),
            relevance: 0,
            language: String::new(),
            illegal: false,
        },
        plaintext_emitter,
    )];
    for name in candidates {
        let Some(key) = resolve_language(&JsString::from(&name)) else {
            continue;
        };
        let Some(language) = compiled(&key) else {
            continue;
        };
        if language.disable_autodetect {
            continue;
        }
        let mut continuations = HashMap::new();
        let result = highlight_compiled(&language, code, false, None, &mut continuations);
        results.push((
            HighlightResult {
                value: result.value,
                relevance: result.relevance,
                language: key,
                illegal: result.illegal,
            },
            result.emitter,
        ));
    }
    results.sort_by(|(left, _), (right, _)| {
        if left.relevance != right.relevance {
            return right.relevance.cmp(&left.relevance);
        }
        let left_superset = superset_of(&left.language);
        let right_superset = superset_of(&right.language);
        if left_superset.as_deref() == Some(right.language.as_str()) {
            return std::cmp::Ordering::Greater;
        }
        if right_superset.as_deref() == Some(left.language.as_str()) {
            return std::cmp::Ordering::Less;
        }
        std::cmp::Ordering::Equal
    });
    results.remove(0)
}

fn superset_of(language: &str) -> Option<String> {
    compiled(language).and_then(|compiled| compiled.superset_of.clone())
}

fn compile_language(entry: LanguageEntry) -> CompiledLanguage {
    let mut language = CompiledLanguage {
        superset_of: entry.superset_of.clone(),
        disable_autodetect: entry.disable_autodetect,
        case_insensitive: entry.case_insensitive,
        class_name_aliases: entry.class_name_aliases.clone(),
        modes: Vec::new(),
        root: 0,
    };
    let root_def = entry.defs.get(&entry.root).cloned().unwrap_or_default();
    let mut context = CompileContext::default();
    let root = compile_mode(&mut language, &entry, &root_def, None, &mut context);
    language.root = root;
    language
}

#[derive(Default)]
struct CompileContext {
    /// Compiled mode index by `(def id, parent)` for modes whose compilation
    /// depends on the parent (`endsWithParent`, including through `starts`).
    /// highlight.js clones exactly those modes per parent; other modes are shared.
    /// Entries stay for the whole language compilation, mirroring highlight.js's
    /// `isCompiled` memoization; an entry may point at a partially compiled mode
    /// while a contains cycle is being expanded.
    compiled: HashMap<(u32, Option<usize>), usize>,
    variant_ids: HashMap<(u32, u32), u32>,
}

fn depends_on_parent(entry: &LanguageEntry, def: &ModeDef) -> bool {
    if def.ends_with_parent {
        return true;
    }
    let Some(ContainsEntry::Mode(reference)) = &def.starts else {
        return false;
    };
    entry
        .defs
        .get(&reference.id)
        .is_some_and(|start| depends_on_parent(entry, start))
}

fn compile_mode(
    language: &mut CompiledLanguage,
    entry: &LanguageEntry,
    def: &ModeDef,
    parent: Option<usize>,
    context: &mut CompileContext,
) -> usize {
    let cache_key = (def.id, depends_on_parent(entry, def).then_some(parent).flatten());
    if let Some(existing) = context.compiled.get(&cache_key) {
        return *existing;
    }
    let index = language.modes.len();
    language.modes.push(CompiledMode::default());
    context.compiled.insert(cache_key, index);

    let mut mode = compiled_mode_metadata(def, parent);
    let (begin, keywords) = resolve_mode_begin(def, parent, &mut mode);
    if parent.is_some() {
        let parent_terminator = parent_terminator(def, parent, language);
        compile_mode_bounds(entry, def, begin, parent_terminator, &mut mode);
    }
    compile_mode_illegal(entry, def, &mut mode);
    compile_mode_keywords(entry, def, keywords, &mut mode);

    // Publish the partially compiled mode so cyclic children can read it.
    language.modes[index] = mode;
    let contains = compile_mode_contains(language, entry, def, index, context);
    let mut mode = std::mem::take(&mut language.modes[index]);
    mode.contains = contains;
    mode.starts = compile_mode_starts(language, entry, def, parent, context);
    mode.rules = build_rules(language, index, &mode);
    language.modes[index] = mode;
    index
}

fn compiled_mode_metadata(def: &ModeDef, parent: Option<usize>) -> CompiledMode {
    CompiledMode {
        class_name: def.class_name.clone(),
        first_parent: parent,
        relevance: def.relevance.unwrap_or(1),
        exclude_begin: def.exclude_begin,
        exclude_end: def.exclude_end,
        return_begin: def.return_begin,
        return_end: def.return_end,
        ends_with_parent: def.ends_with_parent,
        ends_parent: def.ends_parent,
        skip: def.skip,
        sub_language: def.sub_language.clone(),
        on_begin: def.on_begin.as_ref().map(|callback| callback.name.clone()),
        on_end: def.on_end.as_ref().map(|callback| callback.name.clone()),
        end_same_as_begin: def.end_same_as_begin,
        ..CompiledMode::default()
    }
}

/// Resolves `begin`/`match` and `beginKeywords` into the mode's begin pattern.
fn resolve_mode_begin(
    def: &ModeDef,
    parent: Option<usize>,
    mode: &mut CompiledMode,
) -> (Option<Pattern>, Option<Value>) {
    let mut begin = def.begin.clone().or_else(|| def.match_.clone());
    let mut keywords = def.keywords.clone();
    if parent.is_some()
        && let Some(begin_keywords) = &def.begin_keywords
    {
        let alternation = begin_keywords.split(' ').collect::<Vec<_>>().join("|");
        begin = Some(Pattern::Literal(format!("\\b({alternation})(?!\\.)(?=\\b|\\s)")));
        mode.on_begin = Some("skip_if_preceding_dot".to_owned());
        keywords = keywords.or_else(|| Some(Value::String(begin_keywords.clone())));
        if def.relevance.is_none() {
            mode.relevance = 0;
        }
    }
    (begin, keywords)
}

fn parent_terminator<'a>(def: &ModeDef, parent: Option<usize>, language: &'a CompiledLanguage) -> Option<&'a JsString> {
    if !def.ends_with_parent {
        return None;
    }
    parent
        .and_then(|parent| language.modes.get(parent))
        .map(|mode| &mode.terminator_end)
}

fn compile_mode_bounds(
    entry: &LanguageEntry,
    def: &ModeDef,
    begin: Option<Pattern>,
    parent_terminator: Option<&JsString>,
    mode: &mut CompiledMode,
) {
    let begin = begin.unwrap_or(Pattern::Literal("\\B|\\b".to_owned()));
    mode.begin_source = begin.source().into();
    mode.begin = Some(compile_cached_regex(&mode.begin_source, entry.case_insensitive));
    let end = if def.end_same_as_begin {
        Some(begin)
    } else if let Some(end) = &def.end {
        Some(end.clone())
    } else if def.ends_with_parent {
        None
    } else {
        Some(Pattern::Literal("\\B|\\b".to_owned()))
    };
    if let Some(end) = end {
        mode.end_source = end.source().into();
        mode.end = Some(compile_cached_regex(&mode.end_source, entry.case_insensitive));
        mode.terminator_end = end.source().into();
    }
    if let Some(parent_terminator) = parent_terminator.filter(|terminator| !terminator.is_empty()) {
        if !mode.terminator_end.is_empty() {
            mode.terminator_end.push_unit(u16::from(b'|'));
        }
        mode.terminator_end.push_str(parent_terminator);
    }
}

fn compile_mode_illegal(entry: &LanguageEntry, def: &ModeDef, mode: &mut CompiledMode) {
    let Some(illegal) = &def.illegal else {
        return;
    };
    mode.illegal_source = match illegal {
        Illegal::One(pattern) => pattern.source().into(),
        Illegal::Many(patterns) => format!(
            "({})",
            patterns.iter().map(Pattern::source).collect::<Vec<_>>().join("|")
        )
        .into(),
    };
    mode.illegal = Some(compile_cached_regex(&mode.illegal_source, entry.case_insensitive));
}

fn compile_mode_keywords(entry: &LanguageEntry, def: &ModeDef, keywords: Option<Value>, mode: &mut CompiledMode) {
    let (compiled_keywords, keyword_pattern) =
        compile_keywords(keywords.as_ref(), def.lexemes.as_ref(), entry.case_insensitive);
    mode.keywords = compiled_keywords;
    mode.keyword_pattern = keyword_pattern;
}

fn compile_mode_contains(
    language: &mut CompiledLanguage,
    entry: &LanguageEntry,
    def: &ModeDef,
    index: usize,
    context: &mut CompileContext,
) -> Vec<usize> {
    let mut contains = Vec::new();
    for expansion in expand_contains(entry, def, &mut context.variant_ids) {
        match expansion {
            Expansion::SelfRef => contains.push(index),
            Expansion::Def(child) => {
                contains.push(compile_mode(language, entry, &child, Some(index), context));
            }
        }
    }
    contains
}

fn compile_mode_starts(
    language: &mut CompiledLanguage,
    entry: &LanguageEntry,
    def: &ModeDef,
    parent: Option<usize>,
    context: &mut CompileContext,
) -> Option<usize> {
    let Some(ContainsEntry::Mode(reference)) = &def.starts else {
        return None;
    };
    let start_def = entry.defs.get(&reference.id)?.clone();
    Some(compile_mode(language, entry, &start_def, parent, context))
}

enum Expansion {
    SelfRef,
    Def(Box<ModeDef>),
}

/// Expands `contains` the way `[].concat(...mode.contains.map(expandOrCloneMode))` does.
fn expand_contains(entry: &LanguageEntry, def: &ModeDef, variant_ids: &mut HashMap<(u32, u32), u32>) -> Vec<Expansion> {
    let mut expanded = Vec::new();
    for contains in &def.contains {
        expand_entry(entry, contains, variant_ids, &mut expanded);
    }
    expanded
}

fn expand_entry(
    entry: &LanguageEntry,
    contains: &ContainsEntry,
    variant_ids: &mut HashMap<(u32, u32), u32>,
    expanded: &mut Vec<Expansion>,
) {
    match contains {
        ContainsEntry::SelfRef(name) => {
            if name == "self" {
                expanded.push(Expansion::SelfRef);
            }
        }
        ContainsEntry::Group(group) => {
            for nested in group {
                expand_entry(entry, nested, variant_ids, expanded);
            }
        }
        ContainsEntry::Mode(reference) => {
            let Some(child) = entry.defs.get(&reference.id).cloned() else {
                return;
            };
            if child.variants.is_empty() {
                expanded.push(Expansion::Def(Box::new(child)));
                return;
            }
            for variant in &child.variants {
                if let ContainsEntry::Mode(variant_ref) = variant
                    && let Some(variant_def) = entry.defs.get(&variant_ref.id)
                {
                    let id = *variant_ids
                        .entry((child.id, variant_ref.id))
                        .or_insert_with(synthetic_id);
                    expanded.push(Expansion::Def(Box::new(merge_variant(&child, variant_def, id))));
                }
            }
        }
    }
}

/// Builds the synthetic mode `inherit(mode, {variants: null}, variant)`.
fn merge_variant(base: &ModeDef, variant: &ModeDef, id: u32) -> ModeDef {
    let mut merged = base.clone();
    merged.id = id;
    merged.variants = Vec::new();
    merged.class_name = variant.class_name.clone().or(merged.class_name);
    merged.begin = variant.begin.clone().or(merged.begin);
    merged.match_ = variant.match_.clone().or(merged.match_);
    merged.end = variant.end.clone().or(merged.end);
    merged.end_same_as_begin |= variant.end_same_as_begin;
    merged.begin_keywords = variant.begin_keywords.clone().or(merged.begin_keywords);
    merged.lexemes = variant.lexemes.clone().or(merged.lexemes);
    merged.illegal = variant.illegal.clone().or(merged.illegal);
    merged.keywords = variant.keywords.clone().or(merged.keywords);
    if !variant.contains.is_empty() {
        merged.contains = variant.contains.clone();
    }
    merged.relevance = variant.relevance.or(merged.relevance);
    merged.exclude_begin |= variant.exclude_begin;
    merged.exclude_end |= variant.exclude_end;
    merged.return_begin |= variant.return_begin;
    merged.return_end |= variant.return_end;
    merged.ends_with_parent |= variant.ends_with_parent;
    merged.ends_parent |= variant.ends_parent;
    merged.skip |= variant.skip;
    merged.sub_language = variant.sub_language.clone().or(merged.sub_language);
    merged.starts = variant.starts.clone().or(merged.starts);
    merged.on_begin = variant.on_begin.clone().or(merged.on_begin);
    merged.on_end = variant.on_end.clone().or(merged.on_end);
    merged
}

/// Synthetic mode ids for expanded variants, unique within one compilation.
fn synthetic_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1_000_000);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn compile_cached_regex(source: &JsString, case_insensitive: bool) -> Arc<JsRegex> {
    static CACHE: LazyLock<GrammarRegexCache> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
    let key = (source.clone(), case_insensitive);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    cache
        .entry(key)
        .or_insert_with(|| {
            let flags = if case_insensitive { "mi" } else { "m" };
            Arc::new(JsRegex::with_flags(source, flags).expect("frozen highlight grammar regex must compile"))
        })
        .clone()
}

fn compile_keywords(
    raw: Option<&Value>,
    lexemes: Option<&Pattern>,
    case_insensitive: bool,
) -> (CompiledKeywords, Option<Arc<JsRegex>>) {
    let mut compiled = BTreeMap::new();
    let mut pattern = lexemes.map(|pattern| pattern.source().to_owned());
    if let Some(raw) = raw {
        compile_keyword_value(
            raw,
            case_insensitive,
            DEFAULT_KEYWORD_CLASS,
            &mut compiled,
            &mut pattern,
        );
    }
    let pattern = pattern.unwrap_or_else(|| "\\w+".to_owned());
    (compiled, Some(compile_cached_regex(&pattern.into(), case_insensitive)))
}

fn compile_keyword_value(
    raw: &Value,
    case_insensitive: bool,
    class_name: &str,
    compiled: &mut CompiledKeywords,
    pattern: &mut Option<String>,
) {
    match raw {
        Value::String(list) => {
            let words = list.split(' ').collect::<Vec<_>>();
            compile_keyword_list(&words, case_insensitive, class_name, compiled);
        }
        Value::Array(list) => {
            let words = list.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            compile_keyword_list(&words, case_insensitive, class_name, compiled);
        }
        Value::Object(map) => {
            for (key, value) in map {
                if key == "$pattern" {
                    *pattern = keyword_pattern_source(value).or_else(|| pattern.clone());
                    continue;
                }
                compile_keyword_value(value, case_insensitive, key, compiled, pattern);
            }
        }
        _ => {}
    }
}

fn keyword_pattern_source(value: &Value) -> Option<String> {
    if let Some(source) = value.as_str() {
        return Some(source.to_owned());
    }
    value
        .as_object()
        .and_then(|object| object.get("$regex"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn compile_keyword_list(words: &[&str], case_insensitive: bool, class_name: &str, compiled: &mut CompiledKeywords) {
    for word in words {
        let word = if case_insensitive {
            word.to_lowercase()
        } else {
            (*word).to_owned()
        };
        let mut parts = word.split('|');
        let keyword = parts.next().unwrap_or_default();
        let score = match parts.next() {
            Some(provided) => provided.parse::<i64>().unwrap_or(0),
            None => i64::from(!COMMON_KEYWORDS.contains(&keyword.to_lowercase().as_str())),
        };
        compiled.insert(keyword.into(), (class_name.to_owned(), score));
    }
}

fn build_rules(language: &CompiledLanguage, index: usize, mode: &CompiledMode) -> Vec<Rule> {
    let mut rules = Vec::new();
    for child in &mode.contains {
        let Some(child_mode) = language.modes.get(*child) else {
            continue;
        };
        // A `self` child is the mode being built, whose slot still holds a placeholder.
        let pattern = if *child == index {
            mode.begin_source.clone()
        } else {
            child_mode.begin_source.clone()
        };
        rules.push(Rule {
            kind: RuleKind::Begin,
            mode: Some(*child),
            pattern,
        });
    }
    if !mode.terminator_end.is_empty() {
        rules.push(Rule {
            kind: RuleKind::End,
            mode: None,
            pattern: mode.terminator_end.clone(),
        });
    }
    if mode.illegal.is_some() {
        rules.push(Rule {
            kind: RuleKind::Illegal,
            mode: None,
            pattern: mode.illegal_source.clone(),
        });
    }
    rules
}

#[derive(Default)]
struct MatcherState {
    regexes: Vec<Option<Arc<JsRegex>>>,
    regex_index: usize,
}

fn exec_matcher(
    mode: &CompiledMode,
    state: &mut MatcherState,
    code: &JsString,
    last_index: usize,
    case_insensitive: bool,
) -> Option<MatchInfo> {
    let (mut result, mut regex_index) = exec_from(mode, state, code, last_index, state.regex_index, case_insensitive);
    if state.regex_index != 0 {
        let at_position = result.as_ref().is_some_and(|found| found.index == last_index);
        if !at_position {
            let (fallback, _) = exec_from(mode, state, code, last_index + 1, 0, case_insensitive);
            result = fallback;
            regex_index = 0;
        }
    }
    let found = result?;
    let count = mode.rules.iter().filter(|rule| rule.kind == RuleKind::Begin).count();
    let mut next = regex_index + found.position + 1;
    if next == count {
        next = 0;
    }
    state.regex_index = next;
    Some(found)
}

fn exec_from(
    mode: &CompiledMode,
    state: &mut MatcherState,
    code: &JsString,
    last_index: usize,
    regex_index: usize,
    case_insensitive: bool,
) -> (Option<MatchInfo>, usize) {
    if regex_index >= mode.rules.len() {
        return (None, regex_index);
    }
    if state.regexes.len() < mode.rules.len() {
        state.regexes.resize_with(mode.rules.len(), || None);
    }
    let mut best: Option<(usize, usize, MatchInfo)> = None;
    for offset in 0..mode.rules.len() - regex_index {
        let rule_index = regex_index + offset;
        let rule = &mode.rules[rule_index];
        if state.regexes[rule_index].is_none() {
            state.regexes[rule_index] = Some(compile_cached_regex(&rule.pattern, case_insensitive));
        }
        let Some(regex) = state.regexes[rule_index].as_ref() else {
            continue;
        };
        let Some(found) = regex.captures_from(code, last_index) else {
            continue;
        };
        let whole = found.get(0).expect("whole regex match");
        let start = whole.start();
        if best.as_ref().is_some_and(|(best_index, _, _)| start >= *best_index) {
            continue;
        }
        best = Some((
            start,
            offset,
            MatchInfo {
                index: start,
                text: whole.text(),
                groups: (1..found.len())
                    .map(|index| found.get(index).map(|group| group.text()))
                    .collect(),
                kind: rule.kind,
                mode: rule.mode,
                position: offset,
            },
        ));
    }
    match best {
        Some((_, _, matched)) => (Some(matched), regex_index),
        None => (None, regex_index),
    }
}

enum Child {
    Text(JsString),
    Node(usize),
}

struct EmitNode {
    kind: Option<String>,
    sublanguage: bool,
    children: Vec<Child>,
}

struct Emitter {
    nodes: Vec<EmitNode>,
    stack: Vec<usize>,
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    fn new() -> Self {
        Self {
            nodes: vec![EmitNode {
                kind: None,
                sublanguage: false,
                children: Vec::new(),
            }],
            stack: vec![0],
        }
    }

    fn add_text(&mut self, text: &JsString) {
        if text.is_empty() {
            return;
        }
        self.children().push(Child::Text(text.to_owned()));
    }

    fn open_node(&mut self, kind: &str) {
        let index = self.nodes.len();
        self.nodes.push(EmitNode {
            kind: Some(kind.to_owned()),
            sublanguage: false,
            children: Vec::new(),
        });
        self.children().push(Child::Node(index));
        self.stack.push(index);
    }

    fn close_node(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    fn add_keyword(&mut self, text: &JsString, kind: &str) {
        if text.is_empty() {
            return;
        }
        self.open_node(kind);
        self.add_text(text);
        self.close_node();
    }

    fn add_sublanguage(&mut self, mut other: Emitter, name: &str) {
        let offset = self.nodes.len();
        for node in &mut other.nodes {
            for child in &mut node.children {
                if let Child::Node(index) = child {
                    *index += offset;
                }
            }
        }
        let root = &mut other.nodes[0];
        root.kind = (!name.is_empty()).then(|| name.to_owned());
        root.sublanguage = true;
        self.nodes.extend(other.nodes);
        self.children().push(Child::Node(offset));
    }

    fn children(&mut self) -> &mut Vec<Child> {
        let top = *self.stack.last().expect("emitter stack is never empty");
        &mut self.nodes[top].children
    }

    fn to_html(&self) -> JsString {
        let mut output = JsString::default();
        render_node(&self.nodes, 0, &mut output);
        output
    }
}

fn render_node(nodes: &[EmitNode], index: usize, output: &mut JsString) {
    let node = &nodes[index];
    if let Some(kind) = &node.kind {
        if node.sublanguage {
            output.push_str(format!("<span class=\"{kind}\">"));
        } else {
            output.push_str(format!("<span class=\"{CLASS_PREFIX}{kind}\">"));
        }
    }
    for child in &node.children {
        match child {
            Child::Text(text) => output.push_str(escape_html(text)),
            Child::Node(child) => render_node(nodes, *child, output),
        }
    }
    if node.kind.is_some() {
        output.push_str("</span>");
    }
}

struct Highlighted {
    value: JsString,
    relevance: i64,
    illegal: bool,
    language: String,
    top: usize,
    emitter: Emitter,
}

#[derive(Clone, Copy)]
struct Entry {
    mode: usize,
    parent: usize,
}

enum Lexeme {
    Advance(usize),
    Illegal,
}

struct Highlighter<'a> {
    language: &'a CompiledLanguage,
    code: &'a JsString,
    ignore_illegals: bool,
    states: HashMap<usize, MatcherState>,
    continuations: HashMap<String, usize>,
    emitter: Emitter,
    entries: Vec<Entry>,
    mode_buffer: JsString,
    relevance: i64,
    index: usize,
    iterations: u64,
    resume_scan: bool,
    last_match: Option<(RuleKind, usize, JsString)>,
    end_overrides: HashMap<usize, Arc<JsRegex>>,
    begin_data: HashMap<usize, JsString>,
}

fn highlight_compiled(
    language: &CompiledLanguage,
    code: &JsString,
    ignore_illegals: bool,
    continuation: Option<usize>,
    continuations: &mut HashMap<String, usize>,
) -> Highlighted {
    let top = continuation.unwrap_or(language.root);
    let mut highlighter = Highlighter {
        language,
        code,
        ignore_illegals,
        states: HashMap::new(),
        continuations: std::mem::take(continuations),
        emitter: Emitter::new(),
        entries: vec![Entry { mode: top, parent: top }],
        mode_buffer: JsString::default(),
        relevance: 0,
        index: 0,
        iterations: 0,
        resume_scan: false,
        last_match: None,
        end_overrides: HashMap::new(),
        begin_data: HashMap::new(),
    };
    highlighter.process_continuations();
    let result = highlighter.run();
    *continuations = std::mem::take(&mut highlighter.continuations);
    result
}

impl Highlighter<'_> {
    fn run(&mut self) -> Highlighted {
        loop {
            if self.index > self.code.len() {
                break;
            }
            self.iterations += 1;
            if self.resume_scan {
                self.resume_scan = false;
            } else {
                self.state().regex_index = 0;
            }
            let state = self.states.remove(&self.top_mode()).unwrap_or_default();
            let mut state = state;
            let mode = &self.language.modes[self.top_mode()];
            let found = exec_matcher(mode, &mut state, self.code, self.index, self.language.case_insensitive);
            self.states.insert(self.top_mode(), state);
            let Some(found) = found else {
                break;
            };
            let before = self.code.slice(self.index, found.index);
            let found_index = found.index;
            let advance = match self.process_lexeme(&before, Some(found)) {
                Lexeme::Advance(advance) => advance,
                Lexeme::Illegal => return self.illegal_result(),
            };
            self.index = found_index + advance;
            if self.iterations > 100_000 && self.iterations > (self.index as u64) * 3 {
                return self.safe_mode_result();
            }
        }
        let remainder = self.code.slice(self.index, self.code.len());
        if let Lexeme::Illegal = self.process_lexeme(&remainder, None) {
            return self.illegal_result();
        }
        Highlighted {
            value: self.emitter.to_html(),
            relevance: self.relevance,
            illegal: false,
            language: String::new(),
            top: self.top_mode(),
            emitter: std::mem::take(&mut self.emitter),
        }
    }

    fn illegal_result(&mut self) -> Highlighted {
        Highlighted {
            value: escape_html(self.code),
            relevance: 0,
            illegal: true,
            language: String::new(),
            top: self.top_mode(),
            emitter: std::mem::take(&mut self.emitter),
        }
    }

    fn safe_mode_result(&mut self) -> Highlighted {
        Highlighted {
            value: escape_html(self.code),
            relevance: 0,
            illegal: false,
            language: String::new(),
            top: self.top_mode(),
            emitter: std::mem::take(&mut self.emitter),
        }
    }

    fn top_mode(&self) -> usize {
        self.entries.last().expect("entry stack is never empty").mode
    }

    fn state(&mut self) -> &mut MatcherState {
        let top = self.top_mode();
        self.states.entry(top).or_default()
    }

    fn process_lexeme(&mut self, text_before: &JsString, matched: Option<MatchInfo>) -> Lexeme {
        self.mode_buffer.push_str(text_before);
        let Some(matched) = matched else {
            self.process_buffer();
            return Lexeme::Advance(0);
        };
        let lexeme = matched.text.clone();
        if let Some((RuleKind::Begin, index, previous)) = &self.last_match
            && matched.kind == RuleKind::End
            && *index == matched.index
            && lexeme.is_empty()
            && previous.is_empty()
        {
            let length = self.unit_length(matched.index).max(1);
            self.mode_buffer
                .push_str(self.code.slice(matched.index, matched.index + length));
            return Lexeme::Advance(length);
        }
        self.last_match = Some((matched.kind, matched.index, lexeme.clone()));
        match matched.kind {
            RuleKind::Begin => self.do_begin_match(matched),
            RuleKind::Illegal if !self.ignore_illegals => Lexeme::Illegal,
            RuleKind::End => self.do_end_match(matched),
            RuleKind::Illegal => {
                if lexeme.is_empty() {
                    Lexeme::Advance(self.unit_length(matched.index).max(1))
                } else {
                    self.mode_buffer.push_str(&lexeme);
                    Lexeme::Advance(lexeme.len())
                }
            }
        }
    }

    fn unit_length(&self, index: usize) -> usize {
        usize::from(index < self.code.len())
    }

    fn do_begin_match(&mut self, matched: MatchInfo) -> Lexeme {
        let lexeme = matched.text.clone();
        let Some(new_mode) = matched.mode else {
            self.mode_buffer.push_str(&lexeme);
            return Lexeme::Advance(lexeme.len());
        };
        if self.callback_ignores(new_mode, &matched, true) {
            return Lexeme::Advance(self.do_ignore(&lexeme));
        }
        if self.language.modes[new_mode].end_same_as_begin {
            let escaped = escape_regex(&lexeme);
            let regex = compile_cached_regex(&escaped, self.language.case_insensitive);
            self.end_overrides.insert(new_mode, regex);
        }
        if self.language.modes[new_mode].skip {
            self.mode_buffer.push_str(&lexeme);
        } else {
            if self.language.modes[new_mode].exclude_begin {
                self.mode_buffer.push_str(&lexeme);
            }
            self.process_buffer();
            if !self.language.modes[new_mode].return_begin && !self.language.modes[new_mode].exclude_begin {
                self.mode_buffer = lexeme.clone();
            }
        }
        self.start_new_mode(new_mode);
        if self.language.modes[new_mode].return_begin {
            Lexeme::Advance(0)
        } else {
            Lexeme::Advance(lexeme.len())
        }
    }

    fn callback_ignores(&mut self, mode: usize, matched: &MatchInfo, begin: bool) -> bool {
        let callback = if begin {
            self.language.modes[mode].on_begin.clone()
        } else {
            self.language.modes[mode].on_end.clone()
        };
        match callback.as_deref() {
            Some("shebang") => matched.index != 0,
            Some("end_same_begin") => {
                if let Some(group) = matched.groups.first().cloned().flatten() {
                    self.begin_data.insert(mode, group);
                }
                false
            }
            Some("end_same_end") => {
                let group = matched.groups.first().cloned().flatten();
                let expected = self.begin_data.get(&mode).cloned();
                !matches!((group, expected), (Some(group), Some(expected)) if group == expected)
            }
            Some("skip_if_preceding_dot") => {
                matched.index > 0 && self.code.units()[matched.index - 1] == u16::from(b'.')
            }
            Some("jsx_truly_opening_tag") => self.jsx_ignores(matched),
            _ => false,
        }
    }

    fn jsx_ignores(&self, matched: &MatchInfo) -> bool {
        let after = matched.index + matched.text.len();
        match self.code.units().get(after) {
            Some(60) => true,
            Some(62) => {
                let tag = JsString::from("</") + matched.text.slice(1, matched.text.len());
                self.code.find_from(tag, after).is_none()
            }
            _ => false,
        }
    }

    fn do_ignore(&mut self, lexeme: &JsString) -> usize {
        if self.state().regex_index == 0 {
            let length = 1;
            self.mode_buffer.push_str(lexeme.slice(0, length));
            length
        } else {
            self.resume_scan = true;
            0
        }
    }

    fn do_end_match(&mut self, matched: MatchInfo) -> Lexeme {
        let lexeme = matched.text.clone();
        let Some((end_entry, end_mode)) = self.end_of_mode(self.entries.len() - 1, &matched) else {
            return Lexeme::Advance(lexeme.len());
        };
        let end_parent = self.entries[end_entry].parent;
        let origin = self.top_mode();
        self.buffer_end_lexeme(origin, &lexeme);
        self.close_modes_through(end_parent);
        self.start_mode_after_end(end_mode);
        if self.language.modes[origin].return_end {
            Lexeme::Advance(0)
        } else {
            Lexeme::Advance(lexeme.len())
        }
    }

    fn buffer_end_lexeme(&mut self, origin: usize, lexeme: &JsString) {
        if self.language.modes[origin].skip {
            self.mode_buffer.push_str(lexeme);
            return;
        }
        if !(self.language.modes[origin].return_end || self.language.modes[origin].exclude_end) {
            self.mode_buffer.push_str(lexeme);
        }
        self.process_buffer();
        if self.language.modes[origin].exclude_end {
            self.mode_buffer = lexeme.to_owned();
        }
    }

    fn close_modes_through(&mut self, end_parent: usize) {
        loop {
            let mode = self.top_mode();
            if self.language.modes[mode].class_name.is_some() {
                self.emitter.close_node();
            }
            if !self.language.modes[mode].skip && self.language.modes[mode].sub_language.is_none() {
                self.relevance += self.language.modes[mode].relevance;
            }
            if self.entries.len() > 1 {
                self.entries.pop();
            }
            if self.top_mode() == end_parent {
                break;
            }
        }
    }

    fn start_mode_after_end(&mut self, end_mode: usize) {
        let Some(starts) = self.language.modes[end_mode].starts else {
            return;
        };
        if self.language.modes[end_mode].end_same_as_begin
            && let Some(end) = self.end_overrides.get(&end_mode).cloned()
        {
            self.end_overrides.insert(starts, end);
        }
        self.start_new_mode(starts);
    }

    fn end_of_mode(&self, entry: usize, matched: &MatchInfo) -> Option<(usize, usize)> {
        let mode = self.entries[entry].mode;
        let remainder = self.code.slice(matched.index, self.code.len());
        let end = self.end_overrides.get(&mode).or(self.language.modes[mode].end.as_ref());
        let starts_with_end = end.is_some_and(|end| end.find(&remainder).is_some_and(|found| found.start() == 0));
        if starts_with_end {
            if self.language.modes[mode].on_end.is_some() && self.callback_ignores_for(mode, matched) {
                return None;
            }
            let mut current = entry;
            while self.language.modes[self.entries[current].mode].ends_parent {
                if current == 0 {
                    break;
                }
                current -= 1;
            }
            return Some((current, self.entries[current].mode));
        }
        if self.language.modes[mode].ends_with_parent && entry > 0 {
            return self.end_of_mode(entry - 1, matched);
        }
        None
    }

    fn callback_ignores_for(&self, mode: usize, matched: &MatchInfo) -> bool {
        match self.language.modes[mode].on_end.as_deref() {
            Some("end_same_end") => {
                let group = matched.groups.first().cloned().flatten();
                let expected = self.begin_data.get(&mode).cloned();
                !matches!((group, expected), (Some(group), Some(expected)) if group == expected)
            }
            _ => false,
        }
    }

    fn start_new_mode(&mut self, mode: usize) {
        if let Some(class_name) = &self.language.modes[mode].class_name {
            let kind = self
                .language
                .class_name_aliases
                .get(class_name)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            self.emitter.open_node(&kind);
        }
        let parent = self.top_mode();
        self.entries.push(Entry { mode, parent });
    }

    fn process_buffer(&mut self) {
        if self.language.modes[self.top_mode()].sub_language.is_some() {
            self.process_sub_language();
        } else {
            self.process_keywords();
        }
        self.mode_buffer = JsString::default();
    }

    fn process_keywords(&mut self) {
        let language = self.language;
        let mode = &language.modes[self.top_mode()];
        let buffer = std::mem::take(&mut self.mode_buffer);
        if mode.keywords.is_empty() {
            self.emitter.add_text(&buffer);
            return;
        }
        let Some(pattern) = mode.keyword_pattern.as_ref() else {
            self.emitter.add_text(&buffer);
            return;
        };
        let case_insensitive = self.language.case_insensitive;
        let mut last = 0;
        let mut pending = JsString::default();
        for matched in pattern.find_iter(&buffer) {
            let (start, end, text) = (matched.start(), matched.end(), matched.text());
            pending.push_str(buffer.slice(last, start));
            let key = if case_insensitive {
                text.to_lowercase()
            } else {
                text.clone()
            };
            match mode.keywords.get(&key) {
                Some((kind, score)) => {
                    self.emitter.add_text(&pending);
                    pending = JsString::default();
                    self.relevance += score;
                    if kind.starts_with('_') {
                        pending.push_str(&text);
                    } else {
                        let class_name = language
                            .class_name_aliases
                            .get(kind)
                            .cloned()
                            .unwrap_or_else(|| kind.clone());
                        self.emitter.add_keyword(&text, &class_name);
                    }
                }
                None => pending.push_str(&text),
            }
            last = end;
        }
        pending.push_str(buffer.slice(last, buffer.len()));
        self.emitter.add_text(&pending);
    }

    fn process_sub_language(&mut self) {
        if self.mode_buffer.is_empty() {
            return;
        }
        let buffer = std::mem::take(&mut self.mode_buffer);
        let sub_language = self.language.modes[self.top_mode()].sub_language.clone();
        let result = match sub_language {
            Some(SubLanguage::One(name)) => {
                let Some(key) = resolve_language(&JsString::from(&name)) else {
                    self.emitter.add_text(&buffer);
                    return;
                };
                let Some(language) = compiled(&key) else {
                    self.emitter.add_text(&buffer);
                    return;
                };
                let continuation = self.continuations.get(&key).copied();
                let mut continuations = std::mem::take(&mut self.continuations);
                let result = highlight_compiled(&language, &buffer, true, continuation, &mut continuations);
                self.continuations = continuations;
                self.continuations.insert(key, result.top);
                Highlighted {
                    language: name,
                    ..result
                }
            }
            Some(SubLanguage::Many(subset)) => {
                let (auto, emitter) = highlight_auto_internal(
                    &buffer,
                    Some(&subset.into_iter().map(JsString::from).collect::<Vec<_>>()),
                );
                Highlighted {
                    value: auto.value,
                    relevance: auto.relevance,
                    illegal: auto.illegal,
                    language: auto.language,
                    top: self.language.root,
                    emitter,
                }
            }
            None => return,
        };
        if self.language.modes[self.top_mode()].relevance > 0 {
            self.relevance += result.relevance;
        }
        self.emitter.add_sublanguage(result.emitter, &result.language);
    }

    fn process_continuations(&mut self) {
        let language = self.language;
        let mut chain = Vec::new();
        let mut current = self.top_mode();
        while current != language.root {
            if let Some(class_name) = &language.modes[current].class_name {
                chain.push(class_name.clone());
            }
            let Some(parent) = language.modes[current].first_parent else {
                break;
            };
            current = parent;
        }
        for class_name in chain.into_iter().rev() {
            let kind = self
                .language
                .class_name_aliases
                .get(&class_name)
                .cloned()
                .unwrap_or(class_name);
            self.emitter.open_node(&kind);
        }
    }
}

fn escape_regex(value: &JsString) -> JsString {
    static SPECIAL: LazyLock<JsRegex> = LazyLock::new(|| JsRegex::new(r"[-/\\^$*+?.()|\[\]{}]").expect("escape regex"));
    SPECIAL.replace_all(value, r"\$&")
}
