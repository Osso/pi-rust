use crate::shared_oracle as oracle;

use pi_ai::utils::hash::short_hash;
use pi_ai::utils::sanitize_unicode::sanitize_surrogate_units;
use serde_json::json;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("hash-sanitize-oracle.ts", "packages/ai/src");
    compare_hashes(&mut oracle);
    compare_sanitization(&mut oracle);
}

fn compare_hashes(oracle: &mut oracle::TsOracle) {
    let cases = hash_cases();
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "short_hash",
        "cases": cases.iter().map(|text| json!({"name": text, "text": text})).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "hash oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("hash results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, text) in results.iter().zip(cases.iter()) {
        assert_eq!(result["hash"], json!(short_hash(text)), "hash mismatch for {text:?}");
    }
}

fn compare_sanitization(oracle: &mut oracle::TsOracle) {
    let cases = sanitize_cases();
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "sanitize_surrogates",
        "cases": cases.iter().enumerate().map(|(index, units)| json!({"name": index.to_string(), "units": units})).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "sanitize oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("sanitize results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, units) in results.iter().zip(cases.iter()) {
        let actual = sanitize_surrogate_units(units);
        assert_eq!(result["units"], json!(actual), "sanitize mismatch for {units:?}");
    }
}

fn hash_cases() -> Vec<String> {
    let mut cases = vec![
        String::new(),
        "a".to_owned(),
        "abc".to_owned(),
        "hello world".to_owned(),
        "🙈".to_owned(),
        "é".to_owned(),
        "a\0b".to_owned(),
        "The quick brown fox jumps over the lazy dog".to_owned(),
        "0123456789".repeat(20),
        "line\nfeed\ttab\r".to_owned(),
        "{\"json\":[1,2,3]}".to_owned(),
        "call_4VnzVawQXPB9MgYib7CiQFEY|I9b95oN1wD/cHXKTw3PpRkL6KkCtzTJhUxMouMWYwHeTo2j3htzfSk7YPx2vifiIM4g3A8XXyOj8q4Bt6SLUG7gqY1E3ELkrkVQNHglRfUmWj84lqxJY+Puieb3VKyX0FB+83TUzn91cDMF/4gzt990IzqVrc+nIb9RRscRD070Du16q1glydVjWR0SBJsE6TbY/esOjFpqplogQqrajm1eI++f3eLi73R6q7hVusY0QbeFySVxABCjhN0lXB04caBe1rzHjYzul6MAXj7uq+0r17VLq+yrtyYhN12wkmFqHeqTyEei6EFPbMy24Nc+IbJlkP0OCg02W+gOnyBFcbi2ctvJFSOhSjt1CqBdqCnnhwUqXjbWiT0wh3DmLScRgTHmGkaI+oAcQQjfic65nxj+TnEkReA==".to_owned(),
    ];

    for code_point in [0x7f, 0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x10ffff] {
        if let Some(character) = char::from_u32(code_point) {
            cases.push(character.to_string());
        }
    }
    for repeat in 0..40 {
        cases.push("ab🙈é\n".repeat(repeat));
    }

    cases
}

fn sanitize_cases() -> Vec<Vec<u16>> {
    let alphabet = [0x41_u16, 0xd800, 0xd83d, 0xdbff, 0xdc00, 0xde48, 0xdfff, 0x20];
    let mut cases = Vec::new();
    for length in 0..=4 {
        build_sanitize_cases(&alphabet, length, &mut Vec::new(), &mut cases);
    }
    cases
}

fn build_sanitize_cases(alphabet: &[u16], length: usize, current: &mut Vec<u16>, cases: &mut Vec<Vec<u16>>) {
    if current.len() == length {
        cases.push(current.clone());
        return;
    }
    for unit in alphabet {
        current.push(*unit);
        build_sanitize_cases(alphabet, length, current, cases);
        current.pop();
    }
}
