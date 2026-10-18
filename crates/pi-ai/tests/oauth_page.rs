use pi_ai::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};

#[test]
fn renders_the_success_page_with_escaped_message() {
    let html = oauth_success_html("Signed in as a<b>&\"'");
    assert!(html.starts_with("<!doctype html>\n<html lang=\"en\">"));
    assert!(html.contains("<title>Authentication successful</title>"));
    assert!(html.contains("<h1>Authentication successful</h1>"));
    assert!(html.contains("<p>Signed in as a&lt;b&gt;&amp;&quot;&#39;</p>"));
    assert!(!html.contains("class=\"details\""));
}

#[test]
fn renders_the_error_page_with_optional_escaped_details() {
    let html = oauth_error_html("Access denied", Some("code=<bad> & \"quoted\""));
    assert!(html.contains("<title>Authentication failed</title>"));
    assert!(html.contains("<h1>Authentication failed</h1>"));
    assert!(html.contains("<p>Access denied</p>"));
    assert!(html.contains("<div class=\"details\">code=&lt;bad&gt; &amp; &quot;quoted&quot;</div>"));

    let without_details = oauth_error_html("Access denied", None);
    assert!(!without_details.contains("class=\"details\""));

    let empty_details = oauth_error_html("Access denied", Some(""));
    assert!(!empty_details.contains("class=\"details\""));
}

#[test]
fn escapes_ampersands_before_other_entities() {
    let html = oauth_success_html("&lt;");
    assert!(html.contains("<p>&amp;lt;</p>"));
}
