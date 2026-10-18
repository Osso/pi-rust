//! Token model produced by the ported `marked` lexer.
//!
//! Field names and presence match the JSON shape of the pinned `marked`
//! 18.0.5 token stream: absent JavaScript `undefined` values stay absent,
//! while `null` values (link titles, table alignment) serialize as `null`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ListStart {
    Number(u64),
    Empty(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub text: String,
    pub tokens: Vec<Token>,
    pub header: bool,
    pub align: Option<Align>,
    #[serde(skip)]
    pub inline_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDefinition {
    pub href: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListItemType {
    #[serde(rename = "list_item")]
    ListItem,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A token from the ported `marked` lexer.
///
/// `inline_source` holds deferred inline input; it is resolved after the block
/// pass so forward link definitions are visible, mirroring marked's
/// `inlineQueue`, and is never serialized.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Token {
    Space {
        raw: String,
    },
    Code {
        raw: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        #[serde(rename = "codeBlockStyle", skip_serializing_if = "Option::is_none")]
        code_block_style: Option<String>,
        text: String,
    },
    Heading {
        raw: String,
        depth: u8,
        text: String,
        tokens: Vec<Token>,
        #[serde(skip)]
        inline_source: Option<String>,
    },
    Hr {
        raw: String,
    },
    Blockquote {
        raw: String,
        text: String,
        tokens: Vec<Token>,
    },
    List {
        raw: String,
        ordered: bool,
        start: ListStart,
        loose: bool,
        items: Vec<ListItem>,
    },
    Checkbox {
        raw: String,
        checked: bool,
    },
    Html {
        raw: String,
        block: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pre: Option<bool>,
        #[serde(rename = "inLink", skip_serializing_if = "Option::is_none")]
        in_link: Option<bool>,
        #[serde(rename = "inRawBlock", skip_serializing_if = "Option::is_none")]
        in_raw_block: Option<bool>,
        text: String,
    },
    Def {
        raw: String,
        tag: String,
        href: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Table {
        raw: String,
        header: Vec<TableCell>,
        align: Vec<Option<Align>>,
        rows: Vec<Vec<TableCell>>,
    },
    Paragraph {
        raw: String,
        text: String,
        tokens: Vec<Token>,
        #[serde(skip)]
        inline_source: Option<String>,
    },
    Text {
        raw: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        escaped: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<Vec<Token>>,
        #[serde(skip)]
        inline_source: Option<String>,
    },
    Escape {
        raw: String,
        text: String,
    },
    Br {
        raw: String,
    },
    Codespan {
        raw: String,
        text: String,
    },
    Em {
        raw: String,
        text: String,
        tokens: Vec<Token>,
    },
    Strong {
        raw: String,
        text: String,
        tokens: Vec<Token>,
    },
    Del {
        raw: String,
        text: String,
        tokens: Vec<Token>,
    },
    Link {
        raw: String,
        href: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<Option<String>>,
        text: String,
        tokens: Vec<Token>,
    },
    Image {
        raw: String,
        href: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<Option<String>>,
        text: String,
        tokens: Vec<Token>,
    },
    Latex {
        raw: String,
        text: String,
        #[serde(skip_serializing_if = "is_false")]
        pending: bool,
    },
    LatexBlock {
        raw: String,
        text: String,
        #[serde(skip_serializing_if = "is_false")]
        pending: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    #[serde(rename = "type")]
    pub token_type: ListItemType,
    pub raw: String,
    pub task: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub loose: bool,
    pub text: String,
    pub tokens: Vec<Token>,
}

impl Token {
    pub fn has_text(&self) -> Option<&str> {
        match self {
            Token::Code { text, .. }
            | Token::Heading { text, .. }
            | Token::Blockquote { text, .. }
            | Token::Html { text, .. }
            | Token::Paragraph { text, .. }
            | Token::Text { text, .. }
            | Token::Escape { text, .. }
            | Token::Codespan { text, .. }
            | Token::Em { text, .. }
            | Token::Strong { text, .. }
            | Token::Del { text, .. }
            | Token::Link { text, .. }
            | Token::Image { text, .. }
            | Token::Latex { text, .. }
            | Token::LatexBlock { text, .. } => Some(text),
            _ => None,
        }
    }
}
