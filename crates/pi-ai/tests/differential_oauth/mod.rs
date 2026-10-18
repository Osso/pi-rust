#[path = "../support/oracle.rs"]
mod oracle;

use pi_ai::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use pi_ai::auth::oauth::pkce::{challenge_for, generate_pkce};
use serde_json::json;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("oauth-oracle.ts", "packages/ai/src");
    compare_pages(&mut oracle);
    compare_pkce(&mut oracle);
}

fn compare_pages(oracle: &mut oracle::TsOracle) {
    let cases = html_cases();
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "oauth_html",
        "cases": cases.iter().enumerate().map(|(index, case)| case.request(index)).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "oauth html oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("html results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, case) in results.iter().zip(cases.iter()) {
        let actual = match case.details.as_deref() {
            _ if case.kind == "success" => oauth_success_html(&case.message),
            details => oauth_error_html(&case.message, details),
        };
        assert_eq!(result["html"], json!(actual), "html mismatch for {:?}", case.message);
    }
}

fn compare_pkce(oracle: &mut oracle::TsOracle) {
    let response = oracle.request(&json!({"id": 2, "operation": "pkce", "count": 8}));
    assert_eq!(response["ok"], json!(true), "pkce oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("pkce results must be an array");
    assert_eq!(results.len(), 8);
    for result in results {
        let verifier = result["verifier"].as_str().expect("verifier must be a string");
        let challenge = result["challenge"].as_str().expect("challenge must be a string");
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        );
        assert_eq!(challenge_for(verifier), challenge);
    }

    let generated = generate_pkce();
    assert_eq!(generated.challenge, challenge_for(&generated.verifier));
    assert_eq!(generated.verifier.len(), 43);
}

struct HtmlCase {
    kind: &'static str,
    message: String,
    details: Option<String>,
}

impl HtmlCase {
    fn request(&self, index: usize) -> serde_json::Value {
        json!({
            "name": index.to_string(),
            "kind": self.kind,
            "message": self.message,
            "details": self.details,
        })
    }
}

fn success(message: &str) -> HtmlCase {
    HtmlCase {
        kind: "success",
        message: message.to_owned(),
        details: None,
    }
}

fn error(message: &str, details: Option<&str>) -> HtmlCase {
    HtmlCase {
        kind: "error",
        message: message.to_owned(),
        details: details.map(str::to_owned),
    }
}

fn html_cases() -> Vec<HtmlCase> {
    vec![
        success("Signed in. You can close this window."),
        success(""),
        success("a<b>&\"'"),
        success("&lt;"),
        success("emoji 🙈 and é"),
        success("line\nbreak"),
        error("Authentication failed", None),
        error("Access denied", Some("code=<bad> & \"quoted\"")),
        error("Access denied", Some("")),
        error("", Some("details only")),
        error("multi\nline", Some("details\nwith\nlines")),
        error("emoji 🙈", Some("🙈 & <script>alert('x')</script>")),
    ]
}
