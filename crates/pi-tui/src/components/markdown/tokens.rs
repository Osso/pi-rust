//! Token model produced by the ported `marked` lexer.
//!
//! Field names and presence match the JSON shape of the pinned `marked`
//! 18.0.5 token stream: absent JavaScript `undefined` values stay absent,
//! while `null` values (link titles, table alignment) serialize as `null`.

use pi_js_compat::js_json::JsString;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ListStart {
    Number(u64),
    Empty(JsString),
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
    pub text: JsString,
    pub tokens: Vec<Token>,
    pub header: bool,
    pub align: Option<Align>,
    #[serde(skip)]
    pub inline_source: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDefinition {
    pub href: JsString,
    pub title: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ListItemType {
    #[serde(rename = "list_item")]
    ListItem,
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
        raw: JsString,
    },
    Code {
        raw: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<JsString>,
        #[serde(rename = "codeBlockStyle", skip_serializing_if = "Option::is_none")]
        code_block_style: Option<JsString>,
        text: JsString,
    },
    Heading {
        raw: JsString,
        depth: u8,
        text: JsString,
        tokens: Vec<Token>,
        #[serde(skip)]
        inline_source: Option<JsString>,
    },
    Hr {
        raw: JsString,
    },
    Blockquote {
        raw: JsString,
        text: JsString,
        tokens: Vec<Token>,
    },
    List {
        raw: JsString,
        ordered: bool,
        start: ListStart,
        loose: bool,
        items: Vec<ListItem>,
    },
    Checkbox {
        raw: JsString,
        checked: bool,
    },
    Html {
        raw: JsString,
        block: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pre: Option<bool>,
        #[serde(rename = "inLink", skip_serializing_if = "Option::is_none")]
        in_link: Option<bool>,
        #[serde(rename = "inRawBlock", skip_serializing_if = "Option::is_none")]
        in_raw_block: Option<bool>,
        text: JsString,
    },
    Def {
        raw: JsString,
        tag: JsString,
        href: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<JsString>,
    },
    Table {
        raw: JsString,
        header: Vec<TableCell>,
        align: Vec<Option<Align>>,
        rows: Vec<Vec<TableCell>>,
    },
    Paragraph {
        raw: JsString,
        text: JsString,
        tokens: Vec<Token>,
        #[serde(skip)]
        inline_source: Option<JsString>,
    },
    Text {
        raw: JsString,
        text: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        escaped: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<Vec<Token>>,
        #[serde(skip)]
        inline_source: Option<JsString>,
    },
    Escape {
        raw: JsString,
        text: JsString,
    },
    Br {
        raw: JsString,
    },
    Codespan {
        raw: JsString,
        text: JsString,
    },
    Em {
        raw: JsString,
        text: JsString,
        tokens: Vec<Token>,
    },
    Strong {
        raw: JsString,
        text: JsString,
        tokens: Vec<Token>,
    },
    Del {
        raw: JsString,
        text: JsString,
        tokens: Vec<Token>,
    },
    Link {
        raw: JsString,
        href: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<Option<JsString>>,
        text: JsString,
        tokens: Vec<Token>,
    },
    Image {
        raw: JsString,
        href: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<Option<JsString>>,
        text: JsString,
        tokens: Vec<Token>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    #[serde(rename = "type")]
    pub token_type: ListItemType,
    pub raw: JsString,
    pub task: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub loose: bool,
    pub text: JsString,
    pub tokens: Vec<Token>,
}

impl Token {
    pub fn has_text(&self) -> Option<&JsString> {
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
            | Token::Image { text, .. } => Some(text),
            _ => None,
        }
    }
}
