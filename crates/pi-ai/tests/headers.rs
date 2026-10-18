use pi_ai::types::ProviderHeaders;
use pi_ai::utils::headers::{headers_to_record, provider_headers_to_record};

#[test]
fn lowercases_names_and_joins_duplicate_values_like_headers_entries() {
    let entries = vec![
        ("X-Test".to_owned(), "one".to_owned()),
        ("x-test".to_owned(), "two".to_owned()),
        ("Accept".to_owned(), "text/plain".to_owned()),
    ];
    let record = headers_to_record(entries);
    assert_eq!(record["x-test"], "one, two");
    assert_eq!(record["accept"], "text/plain");
}

#[test]
fn provider_headers_drop_null_values_and_empty_maps() {
    let mut headers = ProviderHeaders::new();
    headers.insert("x-keep".to_owned(), Some("value".to_owned()));
    headers.insert("x-drop".to_owned(), None);
    let record = provider_headers_to_record(Some(&headers)).expect("non-empty record");
    assert_eq!(record.len(), 1);
    assert_eq!(record["x-keep"], "value");

    let empty = ProviderHeaders::new();
    assert_eq!(provider_headers_to_record(Some(&empty)), None);
    assert_eq!(provider_headers_to_record(None), None);
}
