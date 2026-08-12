use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{
    oracle::TsOracle,
    support::{Direction, TruncationCase, normalize_options, normalize_result, ok_response, options},
};

const REQUEST_ID: u64 = 10_033;
const MAX_LINES: usize = 10;
const RANDOM_SEED: u32 = 0x1234_5678;

pub(crate) fn compare_corpus(oracle: &mut TsOracle) {
    let cases = corpus_cases();
    assert!(cases.len() > 3_000, "corpus must exercise thousands of boundaries");

    let source_cases = cases.iter().map(normalize_case).collect::<Vec<_>>();
    let expected = oracle.request(&json!({
        "id": REQUEST_ID,
        "operation": "truncate_tail_batch",
        "cases": source_cases,
    }));
    let results = cases
        .iter()
        .map(|case| normalize_result(&case.direction.truncate(&case.content, case.options)))
        .collect::<Vec<_>>();
    let actual = ok_response(REQUEST_ID, json!({"results": results}));

    assert_eq!(actual, expected, "deterministic tail corpus mismatch");
}

fn corpus_cases() -> Vec<TruncationCase> {
    let alphabet = [
        "a", "\u{7f}", "\u{80}", "é", "\u{7ff}", "\u{800}", "中", "\u{d7ff}", "🙂", "\u{e000}", "\u{ffff}",
    ];
    let mut inputs = exhaustive_inputs(&alphabet);
    inputs.extend(random_inputs(&alphabet));

    let mut cases = Vec::new();
    for input in inputs {
        for max_bytes in sampled_byte_limits(&input) {
            cases.push(TruncationCase::new(
                "tail corpus",
                Direction::Tail,
                input.clone(),
                Some(options(MAX_LINES, max_bytes)),
            ));
        }
    }
    cases
}

fn exhaustive_inputs(alphabet: &[&str]) -> Vec<String> {
    let mut inputs = vec![String::new()];
    for first in alphabet {
        inputs.push((*first).to_owned());
        for second in alphabet {
            inputs.push(format!("{first}{second}"));
        }
    }
    inputs
}

fn random_inputs(alphabet: &[&str]) -> Vec<String> {
    let mut seed = RANDOM_SEED;
    let mut inputs = Vec::new();
    for _ in 0..100 {
        seed = next_seed(seed);
        let length = usize::try_from(seed % 40).expect("random length must fit usize");
        let mut input = String::new();
        for _ in 0..length {
            seed = next_seed(seed);
            let index = usize::try_from(seed).expect("random index must fit usize") % alphabet.len();
            input.push_str(alphabet[index]);
        }
        inputs.push(input);
    }
    inputs
}

const fn next_seed(seed: u32) -> u32 {
    seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}

fn sampled_byte_limits(input: &str) -> Vec<usize> {
    let total = input.len();
    let half = total / 2;
    let mut values = BTreeSet::from([0, 1, 2, 3, 4, 5, 8, half, total]);
    insert_checked_sub(&mut values, half, 1);
    values.insert(half.saturating_add(1));
    for difference in [8, 5, 4, 3, 2, 1] {
        insert_checked_sub(&mut values, total, difference);
    }
    values.insert(total.saturating_add(1));
    values.insert(total.saturating_add(4));
    values.into_iter().collect()
}

fn insert_checked_sub(values: &mut BTreeSet<usize>, value: usize, difference: usize) {
    if let Some(candidate) = value.checked_sub(difference) {
        values.insert(candidate);
    }
}

fn normalize_case(case: &TruncationCase) -> Value {
    json!({
        "content": case.content,
        "options": normalize_options(case.options.expect("corpus options must be present")),
    })
}
