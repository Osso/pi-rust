//! Port of `packages/ai/src/cli.ts` as the `pi-ai` binary.
//!
//! Identical argv handling, prompts, stdout/stderr text, and exit codes.
//! Non-interactive EOF while a prompt is pending exits 0 without further
//! output, matching Node's readline (the pending question never resolves).

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_ai::auth::{AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, OAuthCredential};
use pi_ai::models::Provider;
use pi_ai::providers::all::builtin_providers;
use pi_ai::request::{BoxFuture, CallbackError};
use pi_js_compat::number::parse_int;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

const AUTH_FILE: &str = "auth.json";
const PROVIDER_COLUMN_WIDTH: usize = 20;

fn oauth_providers() -> Vec<Arc<dyn Provider>> {
    builtin_providers()
        .into_iter()
        .filter(|provider| provider.auth().oauth.is_some())
        .collect()
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = runtime.block_on(run(std::env::args().skip(1).collect()));
    if let Err(message) = result {
        eprintln!("Error: {message}");
        std::process::exit(1);
    }
}

async fn run(args: Vec<String>) -> Result<(), String> {
    let command = args.first().map(String::as_str);
    match command {
        None | Some("help" | "--help" | "-h") => {
            print_usage();
            Ok(())
        }
        Some("list") => {
            for provider in oauth_providers() {
                println!(
                    "{:<width$} {}",
                    provider.id(),
                    provider.name(),
                    width = PROVIDER_COLUMN_WIDTH
                );
            }
            Ok(())
        }
        Some("login") => login_command(args.get(1).map(String::as_str)).await,
        Some(other) => Err(format!("Unknown command: {other}")),
    }
}

fn print_usage() {
    let providers = oauth_providers();
    let provider_list = providers
        .iter()
        .map(|provider| {
            format!(
                "  {:<width$} {}",
                provider.id(),
                provider.name(),
                width = PROVIDER_COLUMN_WIDTH
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!(
        "Usage: npx @earendil-works/pi-ai <command> [provider]\n\nCommands:\n  login [provider]  Login to an OAuth provider\n  list              List available providers\n\nProviders:\n{provider_list}"
    );
}

async fn login_command(provider_id: Option<&str>) -> Result<(), String> {
    let providers = oauth_providers();
    let provider_id = match provider_id {
        Some(provider_id) => provider_id.to_owned(),
        None => select_provider(&providers)?,
    };
    if !providers.iter().any(|provider| provider.id() == provider_id) {
        return Err(format!("Unknown provider: {provider_id}"));
    }
    login(&providers, &provider_id).await
}

fn select_provider(providers: &[Arc<dyn Provider>]) -> Result<String, String> {
    for (index, provider) in providers.iter().enumerate() {
        println!("  {}. {}", index + 1, provider.name());
    }
    let answer = prompt_line(&format!("Enter number (1-{}): ", providers.len()));
    let choice = parse_int(&answer, 10) - 1.0;
    Ok(array_index(choice, providers.len())
        .map(|index| providers[index].id().to_owned())
        .unwrap_or_default())
}

async fn login(providers: &[Arc<dyn Provider>], provider_id: &str) -> Result<(), String> {
    let provider = providers
        .iter()
        .find(|provider| provider.id() == provider_id)
        .ok_or_else(|| format!("Unknown provider: {provider_id}"))?;
    let oauth = provider
        .auth()
        .oauth
        .as_ref()
        .ok_or_else(|| format!("Unknown provider: {provider_id}"))?;
    let interaction = Arc::new(CliInteraction::new());
    let credential = oauth.login(interaction).await.map_err(|error| error.to_string())?;
    let mut auth = load_auth_from(Path::new(AUTH_FILE));
    auth.insert(provider_id.to_owned(), credential_to_value(&credential));
    save_auth_to(Path::new(AUTH_FILE), &auth)?;
    println!("\nCredentials saved to {AUTH_FILE}");
    Ok(())
}

/// JavaScript array index semantics: only finite, integral, in-range values hit.
fn array_index(value: f64, length: usize) -> Option<usize> {
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value >= length as f64 {
        return None;
    }
    Some(value as usize)
}

fn prompt_line(question: &str) -> String {
    print!("{question}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) => {
            let _ = io::stdout().flush();
            std::process::exit(0);
        }
        Ok(_) => trim_line_ending(line),
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn trim_line_ending(mut line: String) -> String {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    line
}

struct CliInteraction {
    gate: Mutex<()>,
    signal: CancellationToken,
}

impl CliInteraction {
    fn new() -> Self {
        Self {
            gate: Mutex::new(()),
            signal: CancellationToken::new(),
        }
    }
}

impl AuthInteraction for CliInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, CallbackError>> {
        Box::pin(async move {
            let _guard = self.gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            answer_prompt(prompt)
        })
    }

    fn notify(&self, event: AuthEvent) {
        let _guard = self.gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match event {
            AuthEvent::AuthUrl { url, instructions } => {
                println!("\nOpen this URL in your browser:\n{url}");
                if let Some(instructions) = instructions {
                    println!("{instructions}");
                }
            }
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                ..
            } => {
                println!("\nOpen this URL in your browser:\n{verification_uri}");
                println!("Enter code: {user_code}");
            }
            AuthEvent::Info { message, .. } | AuthEvent::Progress { message } => println!("{message}"),
        }
    }
}

fn answer_prompt(prompt: AuthPrompt) -> Result<String, CallbackError> {
    match prompt.kind {
        AuthPromptKind::Select { message, options } => {
            println!("\n{message}");
            for (index, option) in options.iter().enumerate() {
                println!("  {}. {}", index + 1, option.label);
            }
            let answer = prompt_line(&format!("Enter number (1-{}): ", options.len()));
            let choice = parse_int(&answer, 10) - 1.0;
            match array_index(choice, options.len()) {
                Some(index) => Ok(options[index].id.clone()),
                None => Err(CallbackError::new("Invalid selection")),
            }
        }
        AuthPromptKind::Text { message, placeholder }
        | AuthPromptKind::Secret { message, placeholder }
        | AuthPromptKind::ManualCode { message, placeholder } => {
            let suffix = placeholder
                .map(|placeholder| format!(" ({placeholder})"))
                .unwrap_or_default();
            Ok(prompt_line(&format!("{message}{suffix}: ")))
        }
    }
}

fn load_auth_from(path: &Path) -> Map<String, Value> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Map::new();
    };
    match serde_json::from_str(&contents) {
        Ok(Value::Object(auth)) => auth,
        _ => Map::new(),
    }
}

fn save_auth_to(path: &Path, auth: &Map<String, Value>) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(&Value::Object(auth.clone())).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

/// `JSON.stringify(credential)`: token fields plus provider-specific extras.
fn credential_to_value(credential: &OAuthCredential) -> Value {
    let mut object = Map::new();
    object.insert("refresh".to_owned(), Value::String(credential.refresh.clone()));
    object.insert("access".to_owned(), Value::String(credential.access.clone()));
    object.insert("expires".to_owned(), number_value(credential.expires));
    for (key, value) in &credential.extra {
        object.insert(key.clone(), value.clone());
    }
    Value::Object(object)
}

fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_json_matches_the_typescript_shape() {
        let mut credential = OAuthCredential::new("refresh-token", "access-token", 1_700_000_000_000.0);
        credential
            .extra
            .insert("accountId".to_owned(), Value::String("acct".to_owned()));
        assert_eq!(
            credential_to_value(&credential),
            serde_json::json!({
                "refresh": "refresh-token",
                "access": "access-token",
                "expires": 1_700_000_000_000_i64,
                "accountId": "acct",
            })
        );
    }

    #[test]
    fn auth_file_round_trips_in_insertion_order_with_two_space_indent() {
        let path = std::env::temp_dir().join(format!("pi-ai-auth-{}.json", std::process::id()));
        let mut auth = Map::new();
        auth.insert("z-provider".to_owned(), Value::String("1".to_owned()));
        auth.insert("a-provider".to_owned(), Value::String("2".to_owned()));
        save_auth_to(&path, &auth).expect("save auth");
        assert_eq!(
            fs::read_to_string(&path).expect("read auth"),
            "{\n  \"z-provider\": \"1\",\n  \"a-provider\": \"2\"\n}"
        );
        assert_eq!(load_auth_from(&path), auth);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_or_invalid_auth_files_load_empty() {
        let path = std::env::temp_dir().join(format!("pi-ai-auth-missing-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        assert!(load_auth_from(&path).is_empty());

        fs::write(&path, "not json").expect("write invalid auth");
        assert!(load_auth_from(&path).is_empty());

        fs::write(&path, "[1,2]").expect("write non-object auth");
        assert!(load_auth_from(&path).is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn array_index_matches_javascript_indexing() {
        assert_eq!(array_index(f64::NAN, 2), None);
        assert_eq!(array_index(-1.0, 2), None);
        assert_eq!(array_index(0.5, 2), None);
        assert_eq!(array_index(2.0, 2), None);
        assert_eq!(array_index(1.0, 2), Some(1));
        assert_eq!(array_index(-0.0, 2), Some(0));
    }

    #[test]
    fn line_endings_are_trimmed_like_readline() {
        assert_eq!(trim_line_ending("1\n".to_owned()), "1");
        assert_eq!(trim_line_ending("1\r\n".to_owned()), "1");
        assert_eq!(trim_line_ending("1".to_owned()), "1");
        assert_eq!(trim_line_ending("\n".to_owned()), "");
    }
}
