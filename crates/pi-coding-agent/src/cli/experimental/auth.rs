//! Experimental CLI authentication source (`cli/experimental/auth.ts`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthInput {
    Token { token: String },
    File { path: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawAuthOptions {
    pub auth_token: Option<String>,
    pub auth_token_file: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAuthInput {
    pub auth: Option<AuthInput>,
    pub errors: Vec<String>,
}

pub fn parse_auth_input(options: &RawAuthOptions) -> ParsedAuthInput {
    let auth = match (&options.auth_token, &options.auth_token_file) {
        (Some(_), Some(_)) => {
            return ParsedAuthInput {
                auth: None,
                errors: vec!["--auth-token and --auth-token-file are mutually exclusive".to_owned()],
            };
        }
        (Some(token), None) => Some(AuthInput::Token { token: token.clone() }),
        (None, Some(path)) => Some(AuthInput::File { path: path.clone() }),
        (None, None) => None,
    };
    ParsedAuthInput {
        auth,
        errors: Vec::new(),
    }
}
