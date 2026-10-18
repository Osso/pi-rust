//! Experimental CLI transport addresses (`cli/experimental/transport-address.ts`).

use serde::{Deserialize, Serialize};

use crate::utils::git::decode_uri_component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Unix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportAddress {
    pub transport: TransportKind,
    pub path: String,
}

/// Parse `--listen`/`--connect`; the error is the exact source message.
pub fn parse_transport_address(value: &str, option: &str) -> Result<TransportAddress, String> {
    let url = url::Url::parse(value).map_err(|_| invalid_address(option, value))?;
    if url.scheme() != "unix" {
        return Err(format!("Unsupported {option} transport \"{}:\"", url.scheme()));
    }
    if url.host_str().is_some() || url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
        return Err("Unix transport address must not include an authority".to_owned());
    }
    if !value.starts_with("unix:///")
        || value.starts_with("unix:////")
        || value.contains('?')
        || value.contains('#')
        || url.as_str() != value
    {
        return Err(invalid_address(option, value));
    }
    let path = decode_uri_component(url.path()).ok_or_else(|| invalid_address(option, value))?;
    if path.contains('\0') {
        return Err(invalid_address(option, value));
    }
    if !path.starts_with('/') {
        return Err("Unix transport address requires an absolute path".to_owned());
    }
    Ok(TransportAddress {
        transport: TransportKind::Unix,
        path,
    })
}

fn invalid_address(option: &str, value: &str) -> String {
    format!("Invalid {option} address \"{value}\"")
}
