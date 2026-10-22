//! SigV4 signing for Bedrock ConverseStream, matching `@smithy/signature-v4`.
//!
//! This is a Rust-only implementation detail: the TypeScript module delegates
//! signing to the AWS SDK. Byte-for-byte parity is proven against the SDK signer
//! through the differential oracle.

use hmac::{Hmac, Mac};
use pi_js_compat::date::{UtcDateTime, utc_date_time};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use sha2::{Digest, Sha256};

use crate::request::{CallbackError, header_value};

type HmacSha256 = Hmac<Sha256>;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const UNSIGNABLE: [&str; 15] = [
    "authorization",
    "cache-control",
    "connection",
    "expect",
    "from",
    "keep-alive",
    "max-forwards",
    "pragma",
    "referer",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "user-agent",
    "x-amzn-trace-id",
];

/// Resolved AWS credentials used for signing. Values stay exact UTF-16: the
/// secret is UTF-8 encoded for HMAC (`fromUtf8`), the key id and session token
/// become ByteString header values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    pub access_key_id: JsString,
    pub secret_access_key: JsString,
    pub session_token: Option<JsString>,
}

/// Sign an HTTP request in place. `headers` are header pairs; names are matched
/// case-insensitively and the signed output uses lower-case names.
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &mut Vec<(String, String)>,
    body: &[u8],
    credentials: &AwsCredentials,
    scope: (&JsString, &str),
    timestamp_ms: i64,
) -> Result<(), CallbackError> {
    let parts = ParsedUrl::parse(url)?;
    remove_header(headers, "authorization");
    remove_header(headers, "date");
    remove_header(headers, "x-amz-date");

    let (region, service) = scope;
    let (long_date, short_date) = format_amz_date(timestamp_ms);
    set_header(headers, "host", &parts.authority);
    set_header(headers, "x-amz-date", &long_date);
    if let Some(token) = &credentials.session_token {
        set_header(headers, "x-amz-security-token", &header_value(token)?);
    }
    let payload_hash = hex(&Sha256::digest(body));
    if find_header(headers, "x-amz-content-sha256").is_none() {
        set_header(headers, "x-amz-content-sha256", &payload_hash);
    }

    let canonical_headers = canonical_headers(headers);
    let signed_headers = canonical_headers.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{method}\n{}\n\n{}\n\n{signed_headers}\n{payload_hash}",
        canonical_path(&parts.path),
        canonical_headers
            .iter()
            .map(|(name, value)| format!("{name}:{value}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let scope = JsString::from(format!("{short_date}/")) + region + format!("/{service}/aws4_request");
    let string_to_sign = format!(
        "{ALGORITHM}\n{long_date}\n{}\n{}",
        encode_node_utf8(&scope),
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = signing_key(
        &encode_node_utf8(&credentials.secret_access_key),
        &short_date,
        &encode_node_utf8(region),
        service,
    );
    let signature = hex(&hmac(&signing_key, string_to_sign.as_bytes()));
    let authorization = JsString::from(format!("{ALGORITHM} Credential="))
        + &credentials.access_key_id
        + "/"
        + scope
        + format!(", SignedHeaders={signed_headers}, Signature={signature}");
    set_header(headers, "authorization", &header_value(&authorization)?);
    Ok(())
}

struct ParsedUrl {
    authority: String,
    path: String,
}

impl ParsedUrl {
    fn parse(url: &str) -> Result<Self, CallbackError> {
        let (_, rest) = url
            .split_once("://")
            .ok_or_else(|| CallbackError::new("endpoint must be an absolute URL"))?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let path = path.split('?').next().unwrap_or(path);
        if authority.is_empty() {
            return Err(CallbackError::new("endpoint must include a host"));
        }
        Ok(Self {
            authority: authority.to_owned(),
            path: path.to_owned(),
        })
    }
}

fn canonical_headers(headers: &[(String, String)]) -> std::collections::BTreeMap<String, String> {
    let mut canonical = std::collections::BTreeMap::new();
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if UNSIGNABLE.contains(&lower.as_str()) || lower.starts_with("proxy-") || lower.starts_with("sec-") {
            continue;
        }
        canonical.insert(lower, normalize_header_value(value));
    }
    canonical
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some(header) = headers.iter_mut().find(|(key, _)| key.eq_ignore_ascii_case(name)) {
        header.0 = name.to_owned();
        header.1 = value.to_owned();
    } else {
        headers.push((name.to_owned(), value.to_owned()));
    }
}

fn remove_header(headers: &mut Vec<(String, String)>, name: &str) {
    headers.retain(|(key, _)| !key.eq_ignore_ascii_case(name));
}

/// `getCanonicalPath`: escape the already-encoded path once more, then restore `/`.
fn canonical_path(path: &str) -> String {
    aws_escape_uri(path).replace("%2F", "/")
}

/// `escapeUri` (`encodeURIComponent` plus `!'()*`).
pub fn aws_escape_uri(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, short_date: &str, region: &str, service: &str) -> Vec<u8> {
    let date_key = hmac(format!("AWS4{secret}").as_bytes(), short_date.as_bytes());
    let region_key = hmac(&date_key, region.as_bytes());
    let service_key = hmac(&region_key, service.as_bytes());
    hmac(&service_key, b"aws4_request")
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX_LOWER[(byte >> 4) as usize] as char);
        output.push(HEX_LOWER[(byte & 0x0f) as usize] as char);
    }
    output
}

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// `YYYYMMDDTHHMMSSZ` and `YYYYMMDD` for a Unix-millisecond timestamp.
pub fn format_amz_date(timestamp_ms: i64) -> (String, String) {
    let UtcDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        ..
    } = utc_date_time(timestamp_ms);
    let short = format!("{year:04}{month:02}{day:02}");
    let long = format!("{short}T{hour:02}{minute:02}{second:02}Z");
    (long, short)
}
