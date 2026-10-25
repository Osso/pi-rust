use crate::shared_oracle as oracle;

use pi_ai::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use serde_json::json;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("oauth-oracle.ts", "packages/ai/src");
    compare_pages(&mut oracle);
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

struct HtmlCase {
    kind: &'static str,
    message: String,
    details: Option<String>,
}

impl HtmlCase {
    fn request(&self, index: usize) -> serde_json::Value {
        let mut request = json!({
            "name": index.to_string(),
            "kind": self.kind,
            "message": self.message,
        });
        if let Some(details) = &self.details {
            request["details"] = json!(details);
        }
        request
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
        success("Keep __DETAILS__ and __LOGO__ literal"),
        error("__DETAILS__ __LOGO__", Some("Keep __LOGO__ literal")),
        error("Authentication failed", None),
        error("Access denied", Some("code=<bad> & \"quoted\"")),
        error("Access denied", Some("")),
        error("", Some("details only")),
        error("multi\nline", Some("details\nwith\nlines")),
        error("emoji 🙈", Some("🙈 & <script>alert('x')</script>")),
    ]
}
