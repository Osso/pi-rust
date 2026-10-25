use crate::shared_oracle_1 as oracle;

use pi_js_compat::{array_sort::js_array_sort, number::js_number_to_string};
use serde_json::{Value, json};

struct Item {
    id: usize,
    key: f64,
}

fn compare(mode: &str, left: &Item, right: &Item, count: usize) -> f64 {
    match mode {
        "date" => left.key - right.key,
        "boolean" => {
            if left.key > right.key {
                1.0
            } else {
                0.0
            }
        }
        "cycle" => {
            if left.id % 3 == right.id % 3 {
                0.0
            } else if (left.id % 3 + 1) % 3 == right.id % 3 {
                -1.0
            } else {
                1.0
            }
        }
        "alternating" => {
            if count.is_multiple_of(5) {
                f64::NAN
            } else if count.is_multiple_of(2) {
                left.key - right.key
            } else {
                right.key - left.key
            }
        }
        "table" => {
            [f64::NAN, f64::NEG_INFINITY, f64::INFINITY, -0.0, -0.25, 0.25, 0.0][(left.id * 17 + right.id * 31) % 7]
        }
        _ => panic!("unknown mode {mode}"),
    }
}
fn keys(length: usize, pattern: usize) -> Vec<f64> {
    (0..length)
        .map(|index| match pattern {
            0 => {
                if index % 7 == 2 {
                    f64::NAN
                } else {
                    ((index * 37) % 100) as f64
                }
            }
            1 => (index / 3) as f64,
            2 => ((length - index) / 3) as f64,
            3 => {
                if index / 32 % 2 == 0 {
                    (length * 4 - index) as f64
                } else {
                    (index * 4) as f64
                }
            }
            4 => {
                if index < length / 4 {
                    (100_000 + index) as f64
                } else {
                    (index - length / 4) as f64
                }
            }
            5 => {
                if index < length * 3 / 4 {
                    (100_000 + index) as f64
                } else {
                    (index - length * 3 / 4) as f64
                }
            }
            6 | 7 => {
                let half = length / 2;
                let offset = usize::from(index >= half);
                let within = if offset == 0 { index } else { index - half };
                ((within / 16) * 32 + within % 16 + ((offset + pattern - 6) % 2) * 16) as f64
            }
            _ => panic!("unknown pattern"),
        })
        .collect()
}

#[test]
fn pinned_v8_timsort_order_and_comparator_trace_match_inconsistent_functions() {
    let mut oracle = oracle::TsOracle::spawn("array-sort-oracle.mjs", "packages/tui/src");
    let mut cases = 0;
    for length in [
        0, 1, 2, 7, 8, 17, 31, 32, 63, 64, 65, 70, 127, 128, 129, 255, 256, 257, 512, 1024, 2048,
    ] {
        for pattern in 0..8 {
            let keys = keys(length, pattern);
            for mode in ["date", "boolean", "cycle", "alternating", "table"] {
                let expected=oracle.request(&json!({"id":cases,"keys":keys.iter().map(|value|js_number_to_string(*value)).collect::<Vec<_>>(),"mode":mode}));
                assert_eq!(expected["ok"], true, "{expected}");
                let mut values: Vec<_> = keys
                    .iter()
                    .enumerate()
                    .map(|(id, key)| Item { id, key: *key })
                    .collect();
                let mut trace = Vec::<Value>::new();
                js_array_sort(&mut values, |left, right| {
                    let result = compare(mode, left, right, trace.len() + 1);
                    trace.push(json!([left.id, right.id, js_number_to_string(result)]));
                    result
                });
                assert_eq!(
                    json!(values.iter().map(|value| value.id).collect::<Vec<_>>()),
                    expected["order"],
                    "order length={length} pattern={pattern} mode={mode}"
                );
                assert_eq!(
                    json!(trace),
                    expected["trace"],
                    "trace length={length} pattern={pattern} mode={mode}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 840);
}
