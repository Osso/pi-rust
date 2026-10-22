use std::collections::BTreeMap;

use pi_ai::request::HttpRequest;
use pi_ai::types::ProviderEnv;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_once(listener: TcpListener) -> String {
    let (mut socket, _) = listener.accept().await.expect("listener accepts");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = socket.read(&mut chunk).await.expect("request read");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    socket
        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
        .await
        .expect("response write");
    String::from_utf8(buffer).expect("request is UTF-8")
}

#[tokio::test]
async fn proxy_fetch_routes_through_the_selected_proxy() {
    let proxy = TcpListener::bind("127.0.0.1:0").await.expect("proxy bind");
    let proxy_address = proxy.local_addr().expect("proxy address");
    let server = tokio::spawn(serve_once(proxy));

    let proxy_url = format!("http://{proxy_address}");
    let scoped = env(&[("HTTP_PROXY", proxy_url.as_str())]);
    let fetch = pi_ai::utils::node_http_proxy::proxy_fetch(Some(scoped));
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".to_owned(),
                url: "http://example.invalid/path?x=1".to_owned(),
                headers: Vec::new(),
                body: None,
            },
            None,
        )
        .await
        .expect("proxied fetch succeeds");
    assert_eq!(response.status, 200);
    let recorded = server.await.expect("proxy task");
    assert!(
        recorded.starts_with("GET http://example.invalid/path?x=1 HTTP/1.1\r\n"),
        "unexpected proxy request: {recorded}"
    );
}

#[tokio::test]
async fn proxy_fetch_honors_no_proxy_exclusions() {
    let direct = TcpListener::bind("127.0.0.1:0").await.expect("direct bind");
    let direct_address = direct.local_addr().expect("direct address");
    let server = tokio::spawn(serve_once(direct));

    let scoped = env(&[("HTTP_PROXY", "http://127.0.0.1:1"), ("NO_PROXY", "127.0.0.1")]);
    let fetch = pi_ai::utils::node_http_proxy::proxy_fetch(Some(scoped));
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".to_owned(),
                url: format!("http://{direct_address}/direct"),
                headers: Vec::new(),
                body: None,
            },
            None,
        )
        .await
        .expect("direct fetch succeeds");
    assert_eq!(response.status, 200);
    let recorded = server.await.expect("direct task");
    assert!(
        recorded.starts_with("GET /direct HTTP/1.1\r\n"),
        "unexpected direct request: {recorded}"
    );
}

#[tokio::test]
async fn proxy_fetch_reports_invalid_proxy_configuration() {
    let scoped = env(&[("HTTP_PROXY", "socks5://proxy.example:1080")]);
    let fetch = pi_ai::utils::node_http_proxy::proxy_fetch(Some(scoped));
    let error = match fetch
        .fetch(
            HttpRequest {
                method: "GET".to_owned(),
                url: "http://example.invalid/".to_owned(),
                headers: Vec::new(),
                body: None,
            },
            None,
        )
        .await
    {
        Ok(_) => panic!("unsupported proxy must be rejected"),
        Err(error) => error,
    };
    assert!(error.message.contains(UNSUPPORTED_PROXY_PROTOCOL_MESSAGE));
}

use pi_ai::utils::node_http_proxy::{
    ProxyError, UNSUPPORTED_PROXY_PROTOCOL_MESSAGE, resolve_http_proxy_url_for_target_with,
};
use url::Url;

fn env(pairs: &[(&str, &str)]) -> ProviderEnv {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), pi_js_compat::js_json::JsString::from(*value)))
        .collect()
}

fn process(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn resolve(
    target: &str,
    scoped: Option<&ProviderEnv>,
    process_env: &BTreeMap<String, String>,
) -> Result<Option<Url>, ProxyError> {
    resolve_http_proxy_url_for_target_with(target, scoped, |name| process_env.get(name).cloned())
}

#[test]
fn unsupported_proxy_protocol_message_is_pinned() {
    assert_eq!(
        UNSUPPORTED_PROXY_PROTOCOL_MESSAGE,
        "Unsupported proxy protocol. SOCKS and PAC proxy URLs are not supported; use an HTTP or HTTPS proxy URL."
    );
}

#[test]
fn resolves_https_proxy_from_process_environment() {
    let process_env = process(&[("HTTPS_PROXY", "http://proxy.example:8080")]);
    let resolved = resolve("https://bedrock-runtime.us-east-1.amazonaws.com", None, &process_env).expect("valid proxy");
    assert_eq!(
        resolved.map(|url| url.to_string()),
        Some("http://proxy.example:8080/".to_owned())
    );
}

#[test]
fn no_proxy_exclusions_are_honored() {
    let process_env = process(&[
        ("HTTPS_PROXY", "http://proxy.example:8080"),
        ("NO_PROXY", "bedrock-runtime.us-east-1.amazonaws.com"),
    ]);
    assert_eq!(
        resolve("https://bedrock-runtime.us-east-1.amazonaws.com", None, &process_env),
        Ok(None)
    );
    assert_eq!(
        resolve("https://other.example.com", None, &process_env)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://proxy.example:8080/".to_owned())
    );
}

#[test]
fn scoped_proxy_aliases_win_over_process_aliases() {
    let process_env = process(&[("https_proxy", "http://process-proxy.example:8080")]);
    let scoped = env(&[("HTTPS_PROXY", "http://scoped-proxy.example:8080")]);
    assert_eq!(
        resolve(
            "https://bedrock-runtime.us-east-1.amazonaws.com",
            Some(&scoped),
            &process_env
        )
        .expect("valid proxy")
        .map(|url| url.to_string()),
        Some("http://scoped-proxy.example:8080/".to_owned())
    );
}

#[test]
fn socks_and_pac_proxies_are_rejected() {
    let process_env = process(&[("HTTPS_PROXY", "socks5://proxy.example:1080")]);
    assert_eq!(
        resolve("https://example.com", None, &process_env),
        Err(ProxyError::UnsupportedProxyProtocol {
            protocol: "socks5:".to_owned()
        })
    );
    let pac = process(&[("HTTPS_PROXY", "pac+http://proxy.example/proxy.pac")]);
    assert_eq!(
        resolve("https://example.com", None, &pac),
        Err(ProxyError::UnsupportedProxyProtocol {
            protocol: "pac+http:".to_owned()
        })
    );
    assert_eq!(
        ProxyError::UnsupportedProxyProtocol {
            protocol: "socks5:".to_owned()
        }
        .to_string(),
        format!("{UNSUPPORTED_PROXY_PROTOCOL_MESSAGE} Got socks5:")
    );
}

#[test]
fn invalid_proxy_urls_are_reported_with_the_quoted_value() {
    let process_env = process(&[("HTTPS_PROXY", "http://[bad")]);
    match resolve("https://example.com", None, &process_env) {
        Err(ProxyError::InvalidProxyUrl { proxy, message }) => {
            assert_eq!(proxy, "http://[bad");
            assert!(!message.is_empty());
        }
        other => panic!("expected invalid proxy URL, got {other:?}"),
    }
}

#[test]
fn proxy_without_scheme_gets_the_target_protocol() {
    let process_env = process(&[("https_proxy", "proxy.example:8080")]);
    assert_eq!(
        resolve("https://example.com", None, &process_env)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("https://proxy.example:8080/".to_owned())
    );
    let http = process(&[("http_proxy", "proxy.example:3128")]);
    assert_eq!(
        resolve("http://example.com", None, &http)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://proxy.example:3128/".to_owned())
    );
}

#[test]
fn all_proxy_is_the_fallback() {
    let process_env = process(&[("ALL_PROXY", "http://all.example:8888")]);
    assert_eq!(
        resolve("http://example.com", None, &process_env)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://all.example:8888/".to_owned())
    );
    let scoped = env(&[("all_proxy", "http://scoped-all.example:8888")]);
    assert_eq!(
        resolve("https://example.com", Some(&scoped), &process_env)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://scoped-all.example:8888/".to_owned())
    );
}

#[test]
fn no_proxy_supports_wildcards_domains_ports_and_separators() {
    let proxy = "http://proxy.example:8080";
    let cases = [
        ("*", "https://example.com", false),
        ("*.example.com", "https://api.example.com", false),
        (".example.com", "https://api.example.com", false),
        ("api.example.com", "https://api.example.com", false),
        ("api.example.com:443", "https://api.example.com", false),
        ("api.example.com:8443", "https://api.example.com", true),
        ("example.com, other.test", "https://other.test", false),
        ("example.com  other.test", "https://other.test", false),
        ("example.com,,,", "https://api.example.com", true),
        ("example.com", "https://notexample.com", true),
        ("ample.com", "https://example.com", true),
    ];
    for (no_proxy, target, expect_proxy) in cases {
        let process_env = process(&[("HTTPS_PROXY", proxy), ("NO_PROXY", no_proxy)]);
        let resolved = resolve(target, None, &process_env).expect("valid proxy");
        assert_eq!(
            resolved.is_some(),
            expect_proxy,
            "NO_PROXY={no_proxy:?} target={target:?}"
        );
    }
}

#[test]
fn no_proxy_port_matching_uses_protocol_defaults() {
    let process_env = process(&[
        ("HTTP_PROXY", "http://proxy.example:8080"),
        ("NO_PROXY", "example.com:80"),
    ]);
    assert_eq!(resolve("http://example.com", None, &process_env), Ok(None));
    assert!(
        resolve("http://example.com:8081", None, &process_env)
            .expect("valid proxy")
            .is_some()
    );
    let https = process(&[
        ("HTTPS_PROXY", "http://proxy.example:8080"),
        ("NO_PROXY", "example.com:443"),
    ]);
    assert_eq!(resolve("https://example.com", None, &https), Ok(None));
    let explicit_default = process(&[
        ("HTTPS_PROXY", "http://proxy.example:8080"),
        ("NO_PROXY", "example.com:443"),
    ]);
    assert_eq!(resolve("https://example.com:443", None, &explicit_default), Ok(None));
}

#[test]
fn non_http_targets_are_not_proxied() {
    let process_env = process(&[("ALL_PROXY", "http://proxy.example:8080")]);
    assert_eq!(resolve("mailto:user@example.com", None, &process_env), Ok(None));
    assert_eq!(resolve("not a url", None, &process_env), Ok(None));
    assert_eq!(resolve("", None, &process_env), Ok(None));
}

#[test]
fn scoped_empty_values_fall_through_to_process_values() {
    let process_env = process(&[("HTTPS_PROXY", "http://process.example:8080")]);
    let scoped = env(&[("HTTPS_PROXY", "")]);
    assert_eq!(
        resolve("https://example.com", Some(&scoped), &process_env)
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://process.example:8080/".to_owned())
    );
}

#[test]
fn lowercase_scoped_proxy_wins_over_uppercase_scoped_proxy() {
    let scoped = env(&[
        ("https_proxy", "http://lower.example:8080"),
        ("HTTPS_PROXY", "http://upper.example:8080"),
    ]);
    assert_eq!(
        resolve("https://example.com", Some(&scoped), &BTreeMap::new())
            .expect("valid proxy")
            .map(|url| url.to_string()),
        Some("http://lower.example:8080/".to_owned())
    );
}
