//! Glob AST parsing and regular-expression generation (`ast.ts`).

use fancy_regex::Regex;

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
    fn from_char(character: char) -> Option<Self> {
        match character {
            '!' => Some(Self::Bang),
            '?' => Some(Self::Question),
            '@' => Some(Self::At),
            '*' => Some(Self::Star),
            '+' => Some(Self::Plus),
            _ => None,
        }
    }

    fn as_char(self) -> char {
        match self {
            Self::Bang => '!',
            Self::Question => '?',
            Self::At => '@',
            Self::Star => '*',
            Self::Plus => '+',
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AstPart {
    Str(String),
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
const STAR: &str = "[^/]*?";
const STAR_NO_EMPTY: &str = "[^/]+?";

const RE_SPECIALS: &str = "().*{}+?[]^$\\!";
const JUST_DOTS: &[&str] = &["..", "."];
const ADD_PATTERN_START: &[char] = &['[', '.'];

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
    Literal(String),
    Regex(Regex),
    Invalid,
}

impl Ast {
    pub(crate) fn from_glob(pattern: &str, options: &MinimatchOptions) -> Self {
        let characters: Vec<char> = pattern.chars().collect();
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

    fn push_str(&mut self, node: NodeId, value: &str) {
        if !value.is_empty() {
            self.nodes[node].parts.push(AstPart::Str(value.to_owned()));
        }
    }

    fn push_node(&mut self, node: NodeId, child: NodeId) {
        self.nodes[node].parts.push(AstPart::Node(child));
    }

    fn parse_ast(&mut self, characters: &[char], position: usize, node: NodeId, ext_depth: usize) -> usize {
        if self.nodes[node].kind.is_none() {
            return self.parse_plain(characters, position, node, ext_depth);
        }
        self.parse_extglob(characters, position, node, ext_depth)
    }

    fn parse_plain(&mut self, characters: &[char], position: usize, node: NodeId, ext_depth: usize) -> usize {
        let mut index = position;
        let mut accumulated = String::new();
        let mut escaping = false;
        let mut in_brace = false;
        let mut brace_start = 0usize;
        let mut brace_negated = false;
        while index < characters.len() {
            let character = characters[index];
            index += 1;
            if escaping || character == '\\' {
                escaping = !escaping;
                accumulated.push(character);
                continue;
            }
            if in_brace {
                if index == brace_start + 1 {
                    if character == '^' || character == '!' {
                        brace_negated = true;
                    }
                } else if character == ']' && !(index == brace_start + 2 && brace_negated) {
                    in_brace = false;
                }
                accumulated.push(character);
                continue;
            } else if character == '[' {
                in_brace = true;
                brace_start = index;
                brace_negated = false;
                accumulated.push(character);
                continue;
            }
            let recurse = !self.options.noext
                && ExtKind::from_char(character).is_some()
                && characters.get(index) == Some(&'(')
                && ext_depth <= self.options.max_extglob_recursion;
            if recurse {
                self.push_str(node, &accumulated);
                accumulated.clear();
                let ext = self.new_node(ExtKind::from_char(character), Some(node));
                index = self.parse_ast(characters, index, ext, ext_depth + 1);
                self.push_node(node, ext);
                continue;
            }
            accumulated.push(character);
        }
        self.push_str(node, &accumulated);
        index
    }

    fn parse_extglob(&mut self, characters: &[char], position: usize, node: NodeId, ext_depth: usize) -> usize {
        let mut index = position + 1;
        let mut part = self.new_node(None, Some(node));
        let mut parts = Vec::new();
        let mut accumulated = String::new();
        let mut escaping = false;
        let mut in_brace = false;
        let mut brace_start = 0usize;
        let mut brace_negated = false;
        while index < characters.len() {
            let character = characters[index];
            index += 1;
            if escaping || character == '\\' {
                escaping = !escaping;
                accumulated.push(character);
                continue;
            }
            if in_brace {
                if index == brace_start + 1 {
                    if character == '^' || character == '!' {
                        brace_negated = true;
                    }
                } else if character == ']' && !(index == brace_start + 2 && brace_negated) {
                    in_brace = false;
                }
                accumulated.push(character);
                continue;
            } else if character == '[' {
                in_brace = true;
                brace_start = index;
                brace_negated = false;
                accumulated.push(character);
                continue;
            }
            let child_kind = ExtKind::from_char(character);
            let recurse = !self.options.noext
                && child_kind.is_some()
                && characters.get(index) == Some(&'(')
                && (ext_depth <= self.options.max_extglob_recursion || self.can_adopt_type(node, child_kind.unwrap()));
            if recurse {
                let depth_add = usize::from(!self.can_adopt_type(node, child_kind.unwrap()));
                self.push_str(part, &accumulated);
                accumulated.clear();
                let ext = self.new_node(child_kind, Some(part));
                self.push_node(part, ext);
                index = self.parse_ast(characters, index, ext, ext_depth + depth_add);
                continue;
            }
            if character == '|' {
                self.push_str(part, &accumulated);
                accumulated.clear();
                parts.push(part);
                part = self.new_node(None, Some(node));
                continue;
            }
            if character == ')' {
                if accumulated.is_empty() && self.nodes[node].parts.is_empty() {
                    self.nodes[node].empty_ext = true;
                }
                self.push_str(part, &accumulated);
                accumulated.clear();
                for alternative in parts {
                    self.push_node(node, alternative);
                }
                self.push_node(node, part);
                return index;
            }
            accumulated.push(character);
        }
        self.nodes[node].kind = None;
        self.nodes[node].has_magic = None;
        let literal: String = characters[position - 1..].iter().collect();
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
        self.nodes[blank].parts.push(AstPart::Str(String::new()));
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

    fn to_string(&self, node: NodeId) -> String {
        match self.nodes[node].kind {
            Some(kind) => {
                let body = self.nodes[node]
                    .parts
                    .iter()
                    .map(|part| match part {
                        AstPart::Str(value) => value.clone(),
                        AstPart::Node(child) => self.to_string(*child),
                    })
                    .collect::<Vec<_>>()
                    .join("|");
                format!("{}({body})", kind.as_char())
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
            || (self.options.nocase && !self.options.nocase_magic_only && glob.to_uppercase() != glob.to_lowercase());
        if !any_magic {
            return MmPattern::Literal(body);
        }
        let pattern = if self.options.nocase {
            format!("(?i)^{source}$")
        } else {
            format!("^{source}$")
        };
        match Regex::new(&pattern) {
            Ok(regex) => MmPattern::Regex(regex),
            Err(_) => MmPattern::Invalid,
        }
    }

    fn regexp_source(&mut self, node: NodeId, allow_dot: Option<bool>) -> (String, String, bool, bool) {
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
    ) -> (String, String, bool, bool) {
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
        let mut source = String::new();
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
        let mut start = String::new();
        if is_start && let Some(AstPart::Str(first)) = self.nodes[node].parts.first() {
            let dot_traversal_allowed = self.nodes[node].parts.len() == 1 && JUST_DOTS.contains(&first.as_str());
            if !dot_traversal_allowed {
                let first_source = source.chars().next();
                let need_no_traversal = (dot && first_source.is_some_and(|c| ADD_PATTERN_START.contains(&c)))
                    || (source.starts_with("\\.")
                        && source.chars().nth(2).is_some_and(|c| ADD_PATTERN_START.contains(&c)))
                    || (source.starts_with("\\..\\")
                        && source.chars().nth(4).is_some_and(|c| ADD_PATTERN_START.contains(&c)));
                let need_no_dot =
                    !dot && allow_dot != Some(true) && first_source.is_some_and(|c| ADD_PATTERN_START.contains(&c));
                start = if need_no_traversal {
                    START_NO_TRAVERSAL.to_owned()
                } else if need_no_dot {
                    START_NO_DOT.to_owned()
                } else {
                    String::new()
                };
            }
        }
        let mut end = String::new();
        if is_end
            && self.filled_negs
            && self.nodes[node]
                .parent
                .is_some_and(|parent| self.nodes[parent].kind == Some(ExtKind::Bang))
        {
            end = "(?:$|\\/)".to_owned();
        }
        let final_source = format!("{start}{source}{end}");
        (final_source, unescape(&source), has_magic, self.nodes[node].uflag)
    }

    fn extglob_regexp_source(
        &mut self,
        node: NodeId,
        allow_dot: Option<bool>,
        dot: bool,
    ) -> (String, String, bool, bool) {
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
            String::new()
        } else {
            self.parts_to_regexp(node, true)
        };
        if body_dot_allowed == body {
            body_dot_allowed.clear();
        }
        if !body_dot_allowed.is_empty() {
            body = format!("(?:{body})(?:{body_dot_allowed})*?");
        }
        let final_source = if kind == ExtKind::Bang && self.nodes[node].empty_ext {
            let prefix = if self.is_start(node) && !dot { START_NO_DOT } else { "" };
            format!("{prefix}{STAR_NO_EMPTY}")
        } else {
            let close = match kind {
                ExtKind::Bang => {
                    let prefix = if self.is_start(node) && !dot && allow_dot != Some(true) {
                        START_NO_DOT
                    } else {
                        ""
                    };
                    format!(")){prefix}{STAR})")
                }
                ExtKind::At => ")".to_owned(),
                ExtKind::Question => ")?".to_owned(),
                ExtKind::Plus if !body_dot_allowed.is_empty() => ")".to_owned(),
                ExtKind::Star if !body_dot_allowed.is_empty() => ")?".to_owned(),
                ExtKind::Plus => ")+".to_owned(),
                ExtKind::Star => ")*".to_owned(),
            };
            format!("{start}{body}{close}")
        };
        (final_source, unescape(&body), true, self.nodes[node].uflag)
    }

    fn parts_to_regexp(&mut self, node: NodeId, dot: bool) -> String {
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
        parts.join("|")
    }
}

fn parse_glob(glob: &str, has_magic: bool, no_empty: bool) -> (String, String, bool, bool) {
    let characters: Vec<char> = glob.chars().collect();
    let mut escaping = false;
    let mut regex = String::new();
    let mut uflag = false;
    let mut has_magic = has_magic;
    let mut in_star = false;
    let all_stars = !characters.is_empty() && characters.iter().all(|character| *character == '*');
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if escaping {
            escaping = false;
            if RE_SPECIALS.contains(character)
                && matches!(
                    character,
                    '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$'
                )
            {
                regex.push('\\');
            }
            regex.push(character);
            index += 1;
            continue;
        }
        if character == '*' {
            if !in_star {
                in_star = true;
                regex.push_str(if no_empty && all_stars { STAR_NO_EMPTY } else { STAR });
                has_magic = true;
            }
            index += 1;
            continue;
        }
        in_star = false;
        if character == '\\' {
            if index == characters.len() - 1 {
                regex.push_str("\\\\");
            } else {
                escaping = true;
            }
            index += 1;
            continue;
        }
        if character == '[' {
            let parsed = parse_class(&characters, index);
            if parsed.consumed > 0 {
                regex.push_str(&parsed.source);
                uflag = uflag || parsed.needs_uflag;
                index += parsed.consumed;
                has_magic = has_magic || parsed.magic;
                continue;
            }
        }
        if character == '?' {
            regex.push_str(QMARK);
            has_magic = true;
            index += 1;
            continue;
        }
        regex.push_str(&regexp_escape(&character.to_string()));
        index += 1;
    }
    (regex, unescape(glob), has_magic, uflag)
}
