//! SigV4 signing for Bedrock ConverseStream, matching `@smithy/signature-v4`.
//!
//! This is a Rust-only implementation detail: the TypeScript module delegates
//! signing to the AWS SDK. Byte-for-byte parity is proven against the SDK signer
//! through the differential oracle.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

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

/// Resolved AWS credentials used for signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// Signing failure (malformed endpoint or empty credentials).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningError {
    message: String,
}

impl SigningError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for SigningError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SigningError {}

/// Sign an HTTP request in place. `headers` are header pairs; names are matched
/// case-insensitively and the signed output uses lower-case names.
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &mut Vec<(String, String)>,
    body: &[u8],
    credentials: &AwsCredentials,
    scope: (&str, &str),
    timestamp_ms: i64,
) -> Result<(), SigningError> {
    let parts = ParsedUrl::parse(url)?;
    remove_header(headers, "authorization");
    remove_header(headers, "date");
    remove_header(headers, "x-amz-date");

    let (region, service) = scope;
    let (long_date, short_date) = format_amz_date(timestamp_ms);
    set_header(headers, "host", &parts.authority);
    set_header(headers, "x-amz-date", &long_date);
    if let Some(token) = &credentials.session_token {
        set_header(headers, "x-amz-security-token", token);
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
    let scope = format!("{short_date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "{ALGORITHM}\n{long_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = signing_key(&credentials.secret_access_key, &short_date, region, service);
    let signature = hex(&hmac(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );
    set_header(headers, "authorization", &authorization);
    Ok(())
}

struct ParsedUrl {
    authority: String,
    path: String,
}

impl ParsedUrl {
    fn parse(url: &str) -> Result<Self, SigningError> {
        let (_, rest) = url
            .split_once("://")
            .ok_or_else(|| SigningError::new("endpoint must be an absolute URL"))?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let path = path.split('?').next().unwrap_or(path);
        if authority.is_empty() {
            return Err(SigningError::new("endpoint must include a host"));
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
    let seconds = timestamp_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    let short = format!("{year:04}{month:02}{day:02}");
    let long = format!("{short}T{hour:02}{minute:02}{second:02}Z");
    (long, short)
}

/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year, month as u32, day as u32)
}
