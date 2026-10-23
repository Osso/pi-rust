//! Glob AST parsing and regular-expression generation (`ast.ts`).
use super::units::*;
use crate::js_json::JsString;

use crate::js_regex::JsRegex as Regex;

use super::MinimatchOptions;
use super::class::parse_class;
use super::escape::{regexp_escape, unescape};

pub(crate) type NodeId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtKind {
    Bang,
    Question,
    At,
    Star,
    Plus,
}

impl ExtKind {
    fn from_char(character: u16) -> Option<Self> {
        match character {
            BANG => Some(Self::Bang),
            QUESTION => Some(Self::Question),
            AT => Some(Self::At),
            STAR => Some(Self::Star),
            PLUS => Some(Self::Plus),
            _ => None,
        }
    }

    fn as_char(self) -> u16 {
        match self {
            Self::Bang => BANG,
            Self::Question => QUESTION,
            Self::At => AT,
            Self::Star => STAR,
            Self::Plus => PLUS,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AstPart {
    Str(JsString),
    Node(NodeId),
}

#[derive(Clone, Debug)]
pub(crate) struct AstNode {
    pub kind: Option<ExtKind>,
    pub parts: Vec<AstPart>,
    pub parent: Option<NodeId>,
    pub parent_index: usize,
    pub has_magic: Option<bool>,
    pub uflag: bool,
    pub empty_ext: bool,
}

pub(crate) struct Ast {
    nodes: Vec<AstNode>,
    root: NodeId,
    options: MinimatchOptions,
    filled_negs: bool,
    negs: Vec<NodeId>,
}

const START_NO_TRAVERSAL: &str = "(?!(?:^|/)\\.\\.?(?:$|/))";
const START_NO_DOT: &str = "(?!\\.)";
const QMARK: &str = "[^/]";
const STAR_SOURCE: &str = "[^/]*?";
const STAR_NO_EMPTY: &str = "[^/]+?";

const RE_SPECIALS: &str = "().*{}+?[]^$\\!";
const JUST_DOTS: &[&str] = &["..", "."];
const ADD_PATTERN_START: &[u16] = &[OPEN_BRACKET, DOT];

fn adoption_map(kind: ExtKind) -> &'static [ExtKind] {
    match kind {
        ExtKind::Bang => &[ExtKind::At],
        ExtKind::Question => &[ExtKind::Question, ExtKind::At],
        ExtKind::At => &[ExtKind::At],
        ExtKind::Star => &[ExtKind::Star, ExtKind::Plus, ExtKind::Question, ExtKind::At],
        ExtKind::Plus => &[ExtKind::Plus, ExtKind::At],
    }
}

fn adoption_with_space_map(kind: ExtKind) -> &'static [ExtKind] {
    match kind {
        ExtKind::Bang | ExtKind::At => &[ExtKind::Question],
        ExtKind::Plus => &[ExtKind::Question, ExtKind::Star],
        ExtKind::Question | ExtKind::Star => &[],
    }
}

fn adoption_any_map(kind: ExtKind) -> &'static [ExtKind] {
    match kind {
        ExtKind::Bang | ExtKind::Question | ExtKind::At => &[ExtKind::Question, ExtKind::At],
        ExtKind::Star => &[ExtKind::Star, ExtKind::Plus, ExtKind::Question, ExtKind::At],
        ExtKind::Plus => &[ExtKind::Plus, ExtKind::At, ExtKind::Question, ExtKind::Star],
    }
}

fn usurp_map(kind: ExtKind, child: ExtKind) -> Option<ExtKind> {
    match (kind, child) {
        (ExtKind::Bang, ExtKind::Bang) => Some(ExtKind::At),
        (ExtKind::Question, ExtKind::Star | ExtKind::Plus) => Some(ExtKind::Star),
        (ExtKind::At, ExtKind::Bang) => Some(ExtKind::Bang),
        (ExtKind::At, ExtKind::Question) => Some(ExtKind::Question),
        (ExtKind::At, ExtKind::At) => Some(ExtKind::At),
        (ExtKind::At, ExtKind::Star) => Some(ExtKind::Star),
        (ExtKind::At, ExtKind::Plus) => Some(ExtKind::Plus),
        (ExtKind::Plus, ExtKind::Question | ExtKind::Star) => Some(ExtKind::Star),
        _ => None,
    }
}

pub(crate) enum MmPattern {
    Literal(JsString),
    Regex(Regex),
    Invalid,
}

#[derive(Default)]
struct ScanState {
    escaping: bool,
    in_brace: bool,
    brace_start: usize,
    brace_negated: bool,
}

impl ScanState {
    /// Consumes escape and character-class bookkeeping characters. Returns
    /// `true` when the character belongs to the literal accumulator.
    fn consume(&mut self, character: u16, index: usize) -> bool {
        if self.escaping || character == BACKSLASH {
            self.escaping = !self.escaping;
            return true;
        }
        if self.in_brace {
            if index == self.brace_start + 1 {
                if character == CARET || character == BANG {
                    self.brace_negated = true;
                }
            } else if character == CLOSE_BRACKET && !(index == self.brace_start + 2 && self.brace_negated) {
                self.in_brace = false;
            }
            return true;
        }
        if character == OPEN_BRACKET {
            self.in_brace = true;
            self.brace_start = index;
            self.brace_negated = false;
            return true;
        }
        false
    }
}

impl Ast {
    pub(crate) fn from_glob(pattern: &JsString, options: &MinimatchOptions) -> Self {
        let characters: Vec<u16> = pattern.units().to_vec();
        let mut ast = Self {
            nodes: Vec::new(),
            root: 0,
            options: options.clone(),
            filled_negs: false,
            negs: Vec::new(),
        };
        let root = ast.new_node(None, None);
        ast.root = root;
        ast.parse_ast(&characters, 0, root, 0);
        ast
    }

    fn new_node(&mut self, kind: Option<ExtKind>, parent: Option<NodeId>) -> NodeId {
        let parent_index = parent.map_or(0, |parent| self.nodes[parent].parts.len());
        let id = self.nodes.len();
        self.nodes.push(AstNode {
            kind,
            parts: Vec::new(),
            parent,
            parent_index,
            has_magic: kind.map(|_| true),
            uflag: false,
            empty_ext: false,
        });
        if kind == Some(ExtKind::Bang) && !self.filled_negs {
            self.negs.push(id);
        }
        id
    }

    fn push_str(&mut self, node: NodeId, value: &JsString) {
        if !value.is_empty() {
            self.nodes[node].parts.push(AstPart::Str(value.to_owned()));
        }
    }

    fn push_node(&mut self, node: NodeId, child: NodeId) {
        self.nodes[node].parts.push(AstPart::Node(child));
    }

    fn parse_ast(&mut self, characters: &[u16], position: usize, node: NodeId, ext_depth: usize) -> usize {
        if self.nodes[node].kind.is_none() {
            return self.parse_plain(characters, position, node, ext_depth);
        }
        self.parse_extglob(characters, position, node, ext_depth)
    }

    fn parse_plain(&mut self, characters: &[u16], position: usize, node: NodeId, ext_depth: usize) -> usize {
        let mut index = position;
        let mut accumulated = JsString::default();
        let mut scan = ScanState::default();
        while index < characters.len() {
            let character = characters[index];
            index += 1;
            if scan.consume(character, index) {
                accumulated.push_unit(character);
                continue;
            }
            let recurse = !self.options.noext
                && ExtKind::from_char(character).is_some()
                && characters.get(index) == Some(&OPEN_PAREN)
                && ext_depth <= self.options.max_extglob_recursion;
            if recurse {
                self.push_str(node, &accumulated);
                accumulated = JsString::default();
                let ext = self.new_node(ExtKind::from_char(character), Some(node));
                index = self.parse_ast(characters, index, ext, ext_depth + 1);
                self.push_node(node, ext);
                continue;
            }
            accumulated.push_unit(character);
        }
        self.push_str(node, &accumulated);
        index
    }

    fn parse_extglob(&mut self, characters: &[u16], position: usize, node: NodeId, ext_depth: usize) -> usize {
        let mut index = position + 1;
        let mut part = self.new_node(None, Some(node));
        let mut parts = Vec::new();
        let mut accumulated = JsString::default();
        let mut scan = ScanState::default();
        while index < characters.len() {
            let character = characters[index];
            index += 1;
            if scan.consume(character, index) {
                accumulated.push_unit(character);
                continue;
            }
            let child_kind = ExtKind::from_char(character);
            let recurse = !self.options.noext
                && child_kind.is_some()
                && characters.get(index) == Some(&OPEN_PAREN)
                && (ext_depth <= self.options.max_extglob_recursion || self.can_adopt_type(node, child_kind.unwrap()));
            if recurse {
                let depth_add = usize::from(!self.can_adopt_type(node, child_kind.unwrap()));
                self.push_str(part, &accumulated);
                accumulated = JsString::default();
                let ext = self.new_node(child_kind, Some(part));
                self.push_node(part, ext);
                index = self.parse_ast(characters, index, ext, ext_depth + depth_add);
                continue;
            }
            if character == PIPE {
                self.push_str(part, &accumulated);
                accumulated = JsString::default();
                parts.push(part);
                part = self.new_node(None, Some(node));
                continue;
            }
            if character == CLOSE_PAREN {
                if accumulated.is_empty() && self.nodes[node].parts.is_empty() {
                    self.nodes[node].empty_ext = true;
                }
                self.push_str(part, &accumulated);
                for alternative in parts {
                    self.push_node(node, alternative);
                }
                self.push_node(node, part);
                return index;
            }
            accumulated.push_unit(character);
        }
        self.nodes[node].kind = None;
        self.nodes[node].has_magic = None;
        let literal: JsString = characters[position - 1..].iter().collect();
        self.nodes[node].parts = vec![AstPart::Str(literal)];
        index
    }

    fn can_adopt_type(&self, node: NodeId, child: ExtKind) -> bool {
        let Some(kind) = self.nodes[node].kind else {
            return false;
        };
        adoption_any_map(kind).contains(&child)
    }

    fn can_adopt_with_type(&self, node: NodeId, child: ExtKind) -> bool {
        let Some(kind) = self.nodes[node].kind else {
            return false;
        };
        adoption_map(kind).contains(&child)
    }

    fn can_adopt_with_space_type(&self, node: NodeId, child: ExtKind) -> bool {
        let Some(kind) = self.nodes[node].kind else {
            return false;
        };
        adoption_with_space_map(kind).contains(&child)
    }

    fn only_extglob_child(&self, node: NodeId) -> Option<(NodeId, ExtKind)> {
        if self.nodes[node].kind.is_some() || self.nodes[node].parts.len() != 1 {
            return None;
        }
        let AstPart::Node(child) = self.nodes[node].parts[0] else {
            return None;
        };
        let kind = self.nodes[child].kind?;
        Some((child, kind))
    }

    fn can_adopt(&self, node: NodeId, child: NodeId) -> bool {
        self.only_extglob_child(child)
            .is_some_and(|(_, kind)| self.can_adopt_with_type(node, kind))
    }

    fn can_adopt_with_space(&self, node: NodeId, child: NodeId) -> bool {
        self.only_extglob_child(child)
            .is_some_and(|(_, kind)| self.can_adopt_with_space_type(node, kind))
    }

    fn can_usurp(&self, node: NodeId, child: NodeId) -> bool {
        if self.nodes[node].kind.is_none() || self.nodes[node].parts.len() != 1 {
            return false;
        }
        let Some((_, child_kind)) = self.only_extglob_child(child) else {
            return false;
        };
        let kind = self.nodes[node].kind.unwrap();
        usurp_map(kind, child_kind).is_some()
    }

    fn adopt(&mut self, node: NodeId, child: NodeId, index: usize) {
        let AstPart::Node(grandchild) = self.nodes[child].parts[0] else {
            return;
        };
        let moved = std::mem::take(&mut self.nodes[grandchild].parts);
        self.nodes[node].parts.splice(index..index + 1, moved);
        let moved_nodes: Vec<NodeId> = self.nodes[node].parts[index..]
            .iter()
            .filter_map(|part| match part {
                AstPart::Node(moved) => Some(*moved),
                AstPart::Str(_) => None,
            })
            .collect();
        for moved in moved_nodes {
            self.nodes[moved].parent = Some(node);
        }
    }

    fn adopt_with_space(&mut self, node: NodeId, child: NodeId, index: usize) {
        let AstPart::Node(grandchild) = self.nodes[child].parts[0] else {
            return;
        };
        let blank = self.new_node(None, Some(grandchild));
        self.nodes[blank].parts.push(AstPart::Str(JsString::default()));
        self.push_node(grandchild, blank);
        self.adopt(node, child, index);
    }

    fn usurp(&mut self, node: NodeId, child: NodeId) {
        let AstPart::Node(grandchild) = self.nodes[child].parts[0] else {
            return;
        };
        let kind = self.nodes[node].kind.unwrap();
        let child_kind = self.nodes[grandchild].kind.unwrap();
        let Some(new_kind) = usurp_map(kind, child_kind) else {
            return;
        };
        let moved = std::mem::take(&mut self.nodes[grandchild].parts);
        self.nodes[node].parts = moved;
        let moved_nodes: Vec<NodeId> = self.nodes[node]
            .parts
            .iter()
            .filter_map(|part| match part {
                AstPart::Node(moved) => Some(*moved),
                AstPart::Str(_) => None,
            })
            .collect();
        for moved in moved_nodes {
            self.nodes[moved].parent = Some(node);
        }
        self.nodes[node].kind = Some(new_kind);
        self.nodes[node].empty_ext = false;
    }

    fn flatten(&mut self, node: NodeId) {
        if self.nodes[node].kind.is_none() {
            for part in self.nodes[node].parts.clone() {
                if let AstPart::Node(child) = part {
                    self.flatten(child);
                }
            }
            return;
        }
        let mut iterations = 0usize;
        loop {
            let mut done = true;
            let mut index = 0usize;
            while index < self.nodes[node].parts.len() {
                let AstPart::Node(child) = self.nodes[node].parts[index] else {
                    index += 1;
                    continue;
                };
                self.flatten(child);
                if self.can_adopt(node, child) {
                    self.adopt(node, child, index);
                    done = false;
                } else if self.can_adopt_with_space(node, child) {
                    self.adopt_with_space(node, child, index);
                    done = false;
                } else if self.can_usurp(node, child) {
                    self.usurp(node, child);
                    done = false;
                }
                index += 1;
            }
            iterations += 1;
            if done || iterations >= 10 {
                break;
            }
        }
    }

    fn fill_negs(&mut self) {
        if self.filled_negs {
            return;
        }
        self.filled_negs = true;
        while let Some(negative) = self.negs.pop() {
            if self.nodes[negative].kind != Some(ExtKind::Bang) {
                continue;
            }
            let mut current = negative;
            let mut ancestor = self.nodes[current].parent;
            while let Some(ancestor_id) = ancestor {
                if self.nodes[ancestor_id].kind.is_none() {
                    let start = self.nodes[current].parent_index + 1;
                    for index in start..self.nodes[ancestor_id].parts.len() {
                        let target = self.nodes[ancestor_id].parts[index].clone();
                        let alternatives = self.nodes[negative].parts.clone();
                        for alternative in alternatives {
                            let AstPart::Node(part) = alternative else {
                                continue;
                            };
                            match &target {
                                AstPart::Str(value) => {
                                    self.nodes[part].parts.push(AstPart::Str(value.clone()));
                                }
                                AstPart::Node(target_node) => {
                                    let cloned = self.clone_node(*target_node, part);
                                    self.nodes[part].parts.push(AstPart::Node(cloned));
                                }
                            }
                        }
                    }
                }
                current = ancestor_id;
                ancestor = self.nodes[current].parent;
            }
        }
    }

    fn clone_node(&mut self, source: NodeId, parent: NodeId) -> NodeId {
        let kind = self.nodes[source].kind;
        let parts = self.nodes[source].parts.clone();
        let id = self.new_node(kind, Some(parent));
        for part in parts {
            self.copy_in(&part, id);
        }
        id
    }

    fn copy_in(&mut self, part: &AstPart, target: NodeId) {
        match part {
            AstPart::Str(value) => self.push_str(target, value),
            AstPart::Node(source) => {
                let cloned = self.clone_node(*source, target);
                self.push_node(target, cloned);
            }
        }
    }

    fn is_start(&self, node: NodeId) -> bool {
        if node == self.root {
            return true;
        }
        let Some(parent) = self.nodes[node].parent else {
            return false;
        };
        if !self.is_start(parent) {
            return false;
        }
        if self.nodes[node].parent_index == 0 {
            return true;
        }
        for index in 0..self.nodes[node].parent_index {
            let AstPart::Node(part) = &self.nodes[parent].parts[index] else {
                return false;
            };
            if self.nodes[*part].kind != Some(ExtKind::Bang) {
                return false;
            }
        }
        true
    }

    fn is_end(&self, node: NodeId) -> bool {
        if node == self.root {
            return true;
        }
        let Some(parent) = self.nodes[node].parent else {
            return false;
        };
        if self.nodes[parent].kind == Some(ExtKind::Bang) {
            return true;
        }
        if !self.is_end(parent) {
            return false;
        }
        if self.nodes[node].kind.is_none() {
            return self.is_end(parent);
        }
        self.nodes[node].parent_index == self.nodes[parent].parts.len() - 1
    }

    fn has_magic(&self, node: NodeId) -> Option<bool> {
        if let Some(value) = self.nodes[node].has_magic {
            return Some(value);
        }
        for part in &self.nodes[node].parts {
            if let AstPart::Node(child) = part
                && (self.nodes[*child].kind.is_some() || self.has_magic(*child) == Some(true))
            {
                return Some(true);
            }
        }
        None
    }

    fn to_string(&self, node: NodeId) -> JsString {
        match self.nodes[node].kind {
            Some(kind) => {
                let body_parts = self.nodes[node]
                    .parts
                    .iter()
                    .map(|part| match part {
                        AstPart::Str(value) => value.clone(),
                        AstPart::Node(child) => self.to_string(*child),
                    })
                    .collect::<Vec<_>>();
                let body = JsString::join(body_parts, "|");
                JsString::join(
                    [
                        JsString::from_utf16(vec![kind.as_char()]),
                        JsString::from("("),
                        JsString::from(&body),
                        JsString::from(")"),
                    ],
                    "",
                )
            }
            None => self.nodes[node]
                .parts
                .iter()
                .map(|part| match part {
                    AstPart::Str(value) => value.clone(),
                    AstPart::Node(child) => self.to_string(*child),
                })
                .collect(),
        }
    }

    pub(crate) fn mm_pattern(&mut self) -> MmPattern {
        let glob = self.to_string(self.root);
        let (source, body, has_magic, _uflag) = self.regexp_source(self.root, None);
        let any_magic = has_magic
            || self.nodes[self.root].has_magic.unwrap_or(false)
            || (self.options.nocase && !self.options.nocase_magic_only && has_case_variants(&glob));
        if !any_magic {
            return MmPattern::Literal(body);
        }
        let pattern = JsString::from("^") + &source + "$";
        let flags = match (self.options.nocase, _uflag) {
            (true, true) => "iu",
            (true, false) => "i",
            (false, true) => "u",
            (false, false) => "",
        };
        match Regex::with_flags(&pattern, flags) {
            Ok(regex) => MmPattern::Regex(regex),
            Err(_) => MmPattern::Invalid,
        }
    }

    fn regexp_source(&mut self, node: NodeId, allow_dot: Option<bool>) -> (JsString, JsString, bool, bool) {
        let dot = allow_dot.unwrap_or(self.options.dot);
        if node == self.root {
            self.flatten(node);
            self.fill_negs();
        }
        if self.nodes[node].kind.is_none() {
            return self.plain_regexp_source(node, allow_dot, dot);
        }
        self.extglob_regexp_source(node, allow_dot, dot)
    }

    fn plain_regexp_source(
        &mut self,
        node: NodeId,
        allow_dot: Option<bool>,
        dot: bool,
    ) -> (JsString, JsString, bool, bool) {
        let is_start = self.is_start(node);
        let is_end = self.is_end(node);
        let no_empty = is_start
            && is_end
            && !self.nodes[node]
                .parts
                .iter()
                .any(|part| !matches!(part, AstPart::Str(_)));
        let mut has_magic = self.has_magic(node).unwrap_or(false);
        let mut uflag = false;
        let mut source = JsString::default();
        for part in self.nodes[node].parts.clone() {
            match part {
                AstPart::Str(value) => {
                    let (regex, _body, magic, needs_uflag) = parse_glob(&value, has_magic, no_empty);
                    has_magic = magic;
                    uflag = uflag || needs_uflag;
                    source.push_str(&regex);
                }
                AstPart::Node(child) => {
                    let (regex, _body, magic, needs_uflag) = self.regexp_source(child, allow_dot);
                    has_magic = has_magic || magic;
                    uflag = uflag || needs_uflag;
                    source.push_str(&regex);
                }
            }
        }
        self.nodes[node].has_magic = Some(has_magic);
        self.nodes[node].uflag = self.nodes[node].uflag || uflag;
        let start = if is_start {
            self.start_anchor(node, &source, dot, allow_dot)
        } else {
            ""
        };
        let end = if self.is_negative_tail(node, is_end) {
            "(?:$|\\/)"
        } else {
            ""
        };
        let final_source = JsString::join(
            [JsString::from(start), JsString::from(&source), JsString::from(end)],
            "",
        );
        (final_source, unescape(&source), has_magic, self.nodes[node].uflag)
    }

    fn start_anchor(&self, node: NodeId, source: &JsString, dot: bool, allow_dot: Option<bool>) -> &'static str {
        let Some(AstPart::Str(first)) = self.nodes[node].parts.first() else {
            return "";
        };
        let dot_traversal_allowed = self.nodes[node].parts.len() == 1 && JUST_DOTS.iter().any(|dots| first == dots);
        if dot_traversal_allowed {
            return "";
        }
        let first_source = source.units().iter().copied().next();
        let need_no_traversal = (dot && first_source.is_some_and(|c| ADD_PATTERN_START.contains(&c)))
            || (source.starts_with("\\.")
                && source
                    .units()
                    .iter()
                    .copied()
                    .nth(2)
                    .is_some_and(|c| ADD_PATTERN_START.contains(&c)))
            || (source.starts_with("\\..\\")
                && source
                    .units()
                    .iter()
                    .copied()
                    .nth(4)
                    .is_some_and(|c| ADD_PATTERN_START.contains(&c)));
        let need_no_dot =
            !dot && allow_dot != Some(true) && first_source.is_some_and(|c| ADD_PATTERN_START.contains(&c));
        if need_no_traversal {
            START_NO_TRAVERSAL
        } else if need_no_dot {
            START_NO_DOT
        } else {
            ""
        }
    }

    fn is_negative_tail(&self, node: NodeId, is_end: bool) -> bool {
        is_end
            && self.filled_negs
            && self.nodes[node]
                .parent
                .is_some_and(|parent| self.nodes[parent].kind == Some(ExtKind::Bang))
    }

    fn extglob_close(
        &self,
        node: NodeId,
        kind: ExtKind,
        body_dot_allowed: &JsString,
        dot: bool,
        allow_dot: Option<bool>,
    ) -> JsString {
        match kind {
            ExtKind::Bang => {
                let prefix = if self.is_start(node) && !dot && allow_dot != Some(true) {
                    START_NO_DOT
                } else {
                    ""
                };
                JsString::join(
                    [
                        JsString::from("))"),
                        JsString::from(prefix),
                        JsString::from(STAR_SOURCE),
                        JsString::from(")"),
                    ],
                    "",
                )
            }
            ExtKind::At => JsString::from(")"),
            ExtKind::Question => JsString::from(")?"),
            ExtKind::Plus if !body_dot_allowed.is_empty() => JsString::from(")"),
            ExtKind::Star if !body_dot_allowed.is_empty() => JsString::from(")?"),
            ExtKind::Plus => JsString::from(")+"),
            ExtKind::Star => JsString::from(")*"),
        }
    }

    fn extglob_regexp_source(
        &mut self,
        node: NodeId,
        allow_dot: Option<bool>,
        dot: bool,
    ) -> (JsString, JsString, bool, bool) {
        let kind = self.nodes[node].kind.unwrap();
        let repeated = matches!(kind, ExtKind::Star | ExtKind::Plus);
        let start = if kind == ExtKind::Bang { "(?:(?!(?:" } else { "(?:" };
        let mut body = self.parts_to_regexp(node, dot);
        if self.is_start(node) && self.is_end(node) && body.is_empty() && kind != ExtKind::Bang {
            let literal = self.to_string(node);
            self.nodes[node].parts = vec![AstPart::Str(literal.clone())];
            self.nodes[node].kind = None;
            self.nodes[node].has_magic = None;
            return (literal.clone(), unescape(&literal), false, false);
        }
        let mut body_dot_allowed = if !repeated || allow_dot == Some(true) || dot {
            JsString::default()
        } else {
            self.parts_to_regexp(node, true)
        };
        if body_dot_allowed == body {
            body_dot_allowed = JsString::default();
        }
        if !body_dot_allowed.is_empty() {
            body = JsString::join(
                [
                    JsString::from("(?:"),
                    JsString::from(&body),
                    JsString::from(")(?:"),
                    JsString::from(&body_dot_allowed),
                    JsString::from(")*?"),
                ],
                "",
            );
        }
        let final_source = if kind == ExtKind::Bang && self.nodes[node].empty_ext {
            let prefix = if self.is_start(node) && !dot { START_NO_DOT } else { "" };
            JsString::join([JsString::from(prefix), JsString::from(STAR_NO_EMPTY)], "")
        } else {
            let close = self.extglob_close(node, kind, &body_dot_allowed, dot, allow_dot);
            JsString::join([JsString::from(start), JsString::from(&body), close], "")
        };
        (final_source, unescape(&body), true, self.nodes[node].uflag)
    }

    fn parts_to_regexp(&mut self, node: NodeId, dot: bool) -> JsString {
        let is_start = self.is_start(node);
        let is_end = self.is_end(node);
        let mut parts = Vec::new();
        for part in self.nodes[node].parts.clone() {
            let AstPart::Node(child) = part else {
                continue;
            };
            let (regex, _body, _magic, uflag) = self.regexp_source(child, Some(dot));
            self.nodes[node].uflag = self.nodes[node].uflag || uflag;
            if !(is_start && is_end) || !regex.is_empty() {
                parts.push(regex);
            }
        }
        JsString::join(parts, "|")
    }
}

fn parse_glob(glob: &JsString, has_magic: bool, no_empty: bool) -> (JsString, JsString, bool, bool) {
    let characters: Vec<u16> = glob.units().to_vec();
    let mut escaping = false;
    let mut regex = JsString::default();
    let mut uflag = false;
    let mut has_magic = has_magic;
    let mut in_star = false;
    let all_stars = !characters.is_empty() && characters.iter().all(|character| *character == STAR);
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if escaping {
            escaping = false;
            if RE_SPECIALS.encode_utf16().any(|unit| unit == character)
                && matches!(
                    character,
                    BACKSLASH
                        | DOT
                        | PLUS
                        | STAR
                        | QUESTION
                        | OPEN_PAREN
                        | CLOSE_PAREN
                        | OPEN_BRACKET
                        | CLOSE_BRACKET
                        | OPEN_BRACE
                        | CLOSE_BRACE
                        | CARET
                        | DOLLAR
                )
            {
                regex.push_unit(BACKSLASH);
            }
            regex.push_unit(character);
            index += 1;
            continue;
        }
        if character == STAR {
            if !in_star {
                in_star = true;
                regex.push_str(if no_empty && all_stars {
                    STAR_NO_EMPTY
                } else {
                    STAR_SOURCE
                });
                has_magic = true;
            }
            index += 1;
            continue;
        }
        in_star = false;
        if character == BACKSLASH {
            if index == characters.len() - 1 {
                regex.push_str("\\\\");
            } else {
                escaping = true;
            }
            index += 1;
            continue;
        }
        if character == OPEN_BRACKET {
            let parsed = parse_class(&characters, index);
            if parsed.consumed > 0 {
                regex.push_str(&parsed.source);
                uflag = uflag || parsed.needs_uflag;
                index += parsed.consumed;
                has_magic = has_magic || parsed.magic;
                continue;
            }
        }
        if character == QUESTION {
            regex.push_str(QMARK);
            has_magic = true;
            index += 1;
            continue;
        }
        regex.push_str(regexp_escape(&JsString::from_utf16(vec![character])));
        index += 1;
    }
    (regex, unescape(glob), has_magic, uflag)
}

fn has_case_variants(text: &JsString) -> bool {
    char::decode_utf16(text.units().iter().copied())
        .filter_map(Result::ok)
        .any(|point| point.to_uppercase().to_string() != point.to_lowercase().to_string())
}
