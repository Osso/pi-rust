//! Port of `packages/coding-agent/src/extensions/llama/huggingface.ts`.
//!
//! Token discovery, GGUF search, and quantization aggregation for the llama.cpp
//! download flow. All HTTP goes through the injected [`Fetch`] contract.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use indexmap::IndexMap;
use pi_ai::request::{CallbackError, Fetch, HttpRequest};
use pi_ai::types::ProviderEnv;
use pi_js_compat::collator::locale_compare;
use pi_js_compat::number::{js_number_to_string, parse_number};
use pi_js_compat::string::js_trim;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::bundled_extensions::llama::client::{LlamaResult, request_json};
use crate::utils::url_encoding::encode_uri_component;

const DEFAULT_HUGGING_FACE_URL: &str = "https://huggingface.co";
const QUANTIZATION_PATTERN: &str =
    r"(?i)(?:^|[-_.])((?:UD-)?(?:IQ[0-9](?:_[A-Z0-9]+)+|Q[0-9](?:_[A-Z0-9]+)+|BF16|F16|F32|MXFP[0-9](?:_[A-Z0-9]+)*))$";
const SHARD_SUFFIX_PATTERN: &str = r"-[0-9]{5}-of-[0-9]{5}$";
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HuggingFaceModel {
    pub id: String,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub downloads: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HuggingFaceQuantization {
    pub name: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub size: Option<f64>,
}

/// `false | "auto" | "manual"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuggingFaceGated {
    NotGated,
    Auto,
    Manual,
}

impl Serialize for HuggingFaceGated {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::NotGated => serializer.serialize_bool(false),
            Self::Auto => serializer.serialize_str("auto"),
            Self::Manual => serializer.serialize_str("manual"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HuggingFaceModelDetails {
    pub id: String,
    pub gated: HuggingFaceGated,
    pub quantizations: Vec<HuggingFaceQuantization>,
}

fn payload_error(payload: Option<&Value>, fallback: &str) -> String {
    payload
        .and_then(|payload| payload.get("error"))
        .and_then(Value::as_str)
        .filter(|error| !error.is_empty())
        .map_or_else(|| fallback.to_owned(), str::to_owned)
}

/// `/(?:^|;)t=(\d+)/` from the `ratelimit` header.
pub fn parse_rate_limit_delay(value: Option<&str>) -> Option<f64> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| Regex::new(r"(?:^|;)t=([0-9]+)").expect("valid ratelimit pattern"));
    let captures = pattern.captures(value?)?;
    Some(parse_number(&captures[1]))
}

async fn read_token(path: &str) -> Option<String> {
    let token = tokio::fs::read_to_string(path).await.ok()?;
    let token = js_trim(&token);
    (!token.is_empty()).then(|| token.to_owned())
}

fn push_path(paths: &mut Vec<String>, path: Option<String>) {
    if let Some(path) = path
        && !path.is_empty()
        && !paths.contains(&path)
    {
        paths.push(path);
    }
}

fn join_token(directory: &str) -> String {
    Path::new(directory).join("token").to_string_lossy().into_owned()
}

/// `findHuggingFaceToken(env = process.env)`.
///
/// `home` is `os.homedir()`, injected so tests do not read the caller's cache.
pub async fn find_hugging_face_token(env: &ProviderEnv, home: &str) -> Option<String> {
    if let Some(token) = env.get("HF_TOKEN").map(|token| js_trim(token))
        && !token.is_empty()
    {
        return Some(token.to_owned());
    }

    let mut paths = Vec::new();
    push_path(&mut paths, env.get("HF_TOKEN_PATH").cloned());
    push_path(
        &mut paths,
        env.get("HF_HOME")
            .filter(|value| !value.is_empty())
            .map(|value| join_token(value)),
    );
    push_path(
        &mut paths,
        env.get("XDG_CACHE_HOME")
            .filter(|value| !value.is_empty())
            .map(|value| {
                Path::new(value)
                    .join("huggingface/token")
                    .to_string_lossy()
                    .into_owned()
            }),
    );
    push_path(
        &mut paths,
        Some(
            Path::new(home)
                .join(".cache/huggingface/token")
                .to_string_lossy()
                .into_owned(),
        ),
    );

    for path in paths {
        if let Some(token) = read_token(&path).await {
            return Some(token);
        }
    }
    None
}

/// `findHuggingFaceToken()` with `process.env`.
pub async fn find_hugging_face_token_from_process() -> Option<String> {
    let env: ProviderEnv = pi_js_compat::process_env::vars().into_iter().collect();
    let home = pi_js_compat::process_env::get("HOME").unwrap_or_default();
    find_hugging_face_token(&env, &home).await
}

pub struct HuggingFaceClient {
    token: Option<String>,
    base_url: String,
    fetch: Arc<dyn Fetch>,
}

impl HuggingFaceClient {
    pub fn new(token: Option<&str>, base_url: Option<&str>) -> Self {
        Self::with_fetch(token, base_url, pi_ai::http::default_fetch())
    }

    pub fn with_fetch(token: Option<&str>, base_url: Option<&str>, fetch: Arc<dyn Fetch>) -> Self {
        Self {
            token: token.map(str::to_owned),
            base_url: base_url
                .unwrap_or(DEFAULT_HUGGING_FACE_URL)
                .trim_end_matches('/')
                .to_owned(),
            fetch,
        }
    }

    async fn request(&self, path: &str, signal: Option<&CancellationToken>) -> LlamaResult<Option<Value>> {
        let mut headers = Vec::new();
        if let Some(token) = &self.token {
            headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
        }
        let request = HttpRequest {
            method: "GET".to_owned(),
            url: format!("{}{path}", self.base_url),
            headers,
            body: None,
        };
        let (status, headers, payload) = request_json(self.fetch.as_ref(), request, signal).await?;
        if (200..300).contains(&status) {
            return Ok(payload);
        }
        if status == 429 {
            let retry_after = header_value(&headers, "retry-after");
            let delay = parse_number(retry_after.as_deref().unwrap_or(""));
            let delay = if delay != 0.0 && !delay.is_nan() {
                Some(delay)
            } else {
                parse_rate_limit_delay(header_value(&headers, "ratelimit").as_deref())
            };
            return Err(CallbackError::new(match delay {
                Some(delay) => format!(
                    "Hugging Face rate limit reached; retry in {}s",
                    js_number_to_string(delay)
                ),
                None => "Hugging Face rate limit reached".to_owned(),
            }));
        }
        Err(CallbackError::new(payload_error(
            payload.as_ref(),
            &format!("Hugging Face returned HTTP {status}"),
        )))
    }

    pub async fn search(&self, query: &str, signal: Option<&CancellationToken>) -> LlamaResult<Vec<HuggingFaceModel>> {
        let params = pi_ai::auth::oauth::support::form_encode(&[
            ("search", query),
            ("filter", "gguf"),
            ("sort", "downloads"),
            ("direction", "-1"),
            ("limit", "20"),
        ]);
        let payload = self.request(&format!("/api/models?{params}"), signal).await?;
        let Some(entries) = payload.as_ref().and_then(Value::as_array) else {
            return Err(CallbackError::new("Hugging Face returned invalid search results"));
        };
        Ok(entries
            .iter()
            .filter_map(|entry| {
                let id = entry.get("id").and_then(Value::as_str)?;
                Some(HuggingFaceModel {
                    id: id.to_owned(),
                    downloads: entry.get("downloads").and_then(Value::as_f64).unwrap_or(0.0),
                })
            })
            .collect())
    }

    pub async fn details(&self, id: &str, signal: Option<&CancellationToken>) -> LlamaResult<HuggingFaceModelDetails> {
        let encoded_id = id.split('/').map(encode_uri_component).collect::<Vec<_>>().join("/");
        let payload = self
            .request(&format!("/api/models/{encoded_id}?blobs=true"), signal)
            .await?;
        let Some(payload) = payload.filter(|value| value.is_object() || value.is_array()) else {
            return Err(CallbackError::new("Hugging Face returned invalid model details"));
        };

        let mut sizes: IndexMap<String, SizeAggregate> = IndexMap::new();
        if let Some(siblings) = payload.get("siblings").and_then(Value::as_array) {
            for sibling in siblings {
                let Some(filename) = sibling.get("rfilename").and_then(Value::as_str) else {
                    continue;
                };
                if !filename.to_lowercase().ends_with(".gguf") {
                    continue;
                }
                let filename = filename.rsplit('/').next().unwrap_or(filename);
                if filename.to_lowercase().starts_with("mmproj") {
                    continue;
                }
                let stem = &filename[..filename.len() - 5];
                let stem = shard_suffix_pattern().replace(stem, "");
                let Some(quantization) = quantization_pattern()
                    .captures(&stem)
                    .map(|captures| captures[1].to_uppercase())
                else {
                    continue;
                };
                let entry = sizes.entry(quantization).or_insert(SizeAggregate {
                    total: 0.0,
                    complete: true,
                });
                match sibling.get("size").and_then(Value::as_f64) {
                    Some(size) => entry.total += size,
                    None => entry.complete = false,
                }
            }
        }

        let mut quantizations: Vec<HuggingFaceQuantization> = sizes
            .into_iter()
            .map(|(name, aggregate)| HuggingFaceQuantization {
                name,
                size: aggregate.complete.then_some(aggregate.total),
            })
            .collect();
        quantizations.sort_by(compare_quantizations);

        Ok(HuggingFaceModelDetails {
            id: payload
                .get("id")
                .and_then(Value::as_str)
                .map_or_else(|| id.to_owned(), str::to_owned),
            gated: match payload.get("gated").and_then(Value::as_str) {
                Some("auto") => HuggingFaceGated::Auto,
                Some("manual") => HuggingFaceGated::Manual,
                _ => HuggingFaceGated::NotGated,
            },
            quantizations,
        })
    }
}

struct SizeAggregate {
    total: f64,
    complete: bool,
}

fn quantization_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(QUANTIZATION_PATTERN).expect("valid quantization pattern"))
}

fn shard_suffix_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(SHARD_SUFFIX_PATTERN).expect("valid shard pattern"))
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

fn compare_quantizations(left: &HuggingFaceQuantization, right: &HuggingFaceQuantization) -> std::cmp::Ordering {
    if left.name == "Q4_K_M" {
        return std::cmp::Ordering::Less;
    }
    if right.name == "Q4_K_M" {
        return std::cmp::Ordering::Greater;
    }
    let difference = left.size.unwrap_or(MAX_SAFE_INTEGER) - right.size.unwrap_or(MAX_SAFE_INTEGER);
    if difference != 0.0 && !difference.is_nan() {
        return if difference < 0.0 {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        };
    }
    locale_compare(&left.name, &right.name)
}
