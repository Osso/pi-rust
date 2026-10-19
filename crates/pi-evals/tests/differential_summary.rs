//! Differential tests for `evals/src/vitest-evals/summary.ts`.

#[path = "support/oracle.rs"]
mod support;

use std::ffi::OsString;

use pi_coding_agent::utils::js_json;
use pi_evals::style::StyleText;
use pi_evals::vitest_evals::summary::{
    HarnessComparisonReport, HarnessObservation, HarnessObservationBase, HarnessObservationOutcome,
    format_harness_comparison_report_with_style, summarize_harness_comparisons,
};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/evals/src/vitest-evals/summary.ts";

fn observation(
    harness: &str,
    test_name: &str,
    result: &str,
    metrics: Option<(Option<f64>, Option<f64>, Option<f64>)>,
    baseline: &str,
    candidates: &[&str],
) -> HarnessObservation {
    let (total_tokens, total_ms, estimated_cost_usd) = metrics.unwrap_or((None, None, None));
    let (outcome, score) = match result {
        "passed" => (HarnessObservationOutcome::Scored, Some(1.0)),
        "failed" => (HarnessObservationOutcome::Scored, Some(0.0)),
        "errored" => (HarnessObservationOutcome::Errored, None),
        "unscored" => (HarnessObservationOutcome::Unscored, None),
        "skipped" => (HarnessObservationOutcome::Skipped, None),
        "pending" => (HarnessObservationOutcome::Pending, None),
        other => panic!("unknown observation result {other}"),
    };
    HarnessObservation {
        base: HarnessObservationBase {
            eval_set: "tool access".to_owned(),
            group_key: js_json::stringify(&json!([test_name, 1])),
            test_name: test_name.to_owned(),
            file: "src/tool-access.eval.ts".to_owned(),
            harness: harness.to_owned(),
            baseline: baseline.to_owned(),
            candidates: candidates.iter().map(|name| (*name).to_owned()).collect(),
            repetition: 1.0,
            total_tokens,
            total_ms,
            estimated_cost_usd,
        },
        outcome,
        score,
    }
}

fn default_observation(harness: &str, test_name: &str, result: &str) -> HarnessObservation {
    observation(harness, test_name, result, None, "without-tools", &["with-tools"])
}

fn fixtures() -> Vec<(&'static str, Vec<HarnessObservation>)> {
    let mut identical_files = vec![
        default_observation("without-tools", "shared", "failed"),
        default_observation("with-tools", "shared", "passed"),
    ];
    let mut other_file = default_observation("without-tools", "shared", "passed");
    other_file.base.file = "src/other.eval.ts".to_owned();
    let mut other_candidate = default_observation("with-tools", "shared", "passed");
    other_candidate.base.file = "src/other.eval.ts".to_owned();
    identical_files.push(other_file);
    identical_files.push(other_candidate);

    let three_harnesses = ["second", "third"];
    let duplicates = vec![
        observation("first", "duplicate", "passed", None, "first", &three_harnesses),
        observation("first", "duplicate", "failed", None, "first", &three_harnesses),
        observation("second", "duplicate", "passed", None, "first", &three_harnesses),
        observation("third", "duplicate", "passed", None, "first", &three_harnesses),
        observation("first", "skipped", "skipped", None, "first", &three_harnesses),
        observation("second", "skipped", "passed", None, "first", &three_harnesses),
        observation("third", "skipped", "passed", None, "first", &three_harnesses),
    ];

    let mut sorted_eval_sets = vec![
        default_observation("without-tools", "zeta", "passed"),
        default_observation("with-tools", "zeta", "passed"),
        default_observation("without-tools", "alpha", "passed"),
        default_observation("with-tools", "alpha", "passed"),
    ];
    for observation in &mut sorted_eval_sets {
        observation.base.eval_set = if observation.base.test_name == "zeta" {
            "zeta set".to_owned()
        } else {
            "alpha set".to_owned()
        };
    }

    let mut name_ties = vec![
        observation("baseline", "input", "passed", None, "baseline", &["charlie", "bravo"]),
        observation("bravo", "input", "passed", None, "baseline", &["charlie", "bravo"]),
        observation("charlie", "input", "passed", None, "baseline", &["charlie", "bravo"]),
    ];
    name_ties[0].base.candidates = vec!["charlie".to_owned(), "bravo".to_owned()];

    vec![
        (
            "paired correctness and efficiency",
            vec![
                observation(
                    "without-tools",
                    "create",
                    "failed",
                    Some((Some(100.0), Some(1000.0), Some(0.01))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "create",
                    "passed",
                    Some((Some(120.0), Some(800.0), Some(0.02))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "without-tools",
                    "inspect",
                    "passed",
                    Some((Some(200.0), None, None)),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "inspect",
                    "passed",
                    Some((Some(180.0), None, None)),
                    "without-tools",
                    &["with-tools"],
                ),
            ],
        ),
        (
            "missing observations",
            vec![
                default_observation("without-tools", "create", "failed"),
                default_observation("with-tools", "create", "passed"),
                default_observation("without-tools", "inspect", "passed"),
            ],
        ),
        ("identical inputs in different files", identical_files),
        (
            "harness errors",
            vec![
                observation(
                    "without-tools",
                    "create",
                    "errored",
                    Some((Some(100.0), None, None)),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "create",
                    "passed",
                    Some((Some(100.0), None, None)),
                    "without-tools",
                    &["with-tools"],
                ),
            ],
        ),
        (
            "unscored observations",
            vec![
                default_observation("without-tools", "create", "unscored"),
                default_observation("with-tools", "create", "unscored"),
            ],
        ),
        (
            "multiple candidates",
            vec![
                observation("first", "input", "passed", None, "first", &three_harnesses),
                observation("second", "input", "passed", None, "first", &three_harnesses),
                observation("third", "input", "passed", None, "first", &three_harnesses),
            ],
        ),
        (
            "declared harness without observations",
            vec![default_observation("without-tools", "create", "failed")],
        ),
        ("duplicate and unscorable", duplicates),
        (
            "formatting lift and telemetry",
            vec![
                observation(
                    "without-tools",
                    "create",
                    "failed",
                    Some((None, Some(34853.7), None)),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "create",
                    "passed",
                    Some((None, Some(30694.2), None)),
                    "without-tools",
                    &["with-tools"],
                ),
            ],
        ),
        ("empty", Vec::new()),
        ("sorted eval sets", sorted_eval_sets),
        ("candidate name ties", name_ties),
        (
            "partial metric coverage and rounding",
            vec![
                observation(
                    "without-tools",
                    "first",
                    "passed",
                    Some((Some(3.0), Some(1.25), Some(-0.005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "first",
                    "passed",
                    Some((Some(4.0), Some(2.5), Some(0.005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "without-tools",
                    "second",
                    "passed",
                    Some((Some(1.0), Some(1.0), Some(1.0))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "second",
                    "passed",
                    Some((Some(1.0), Some(1.0), Some(1.0))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "without-tools",
                    "third",
                    "passed",
                    Some((Some(1.0), Some(1.0), Some(1.0))),
                    "without-tools",
                    &["with-tools"],
                ),
            ],
        ),
        (
            "zero deltas and fractional pass rates",
            vec![
                observation(
                    "without-tools",
                    "one",
                    "passed",
                    Some((Some(10.0), Some(10.0), Some(0.00005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "one",
                    "passed",
                    Some((Some(10.0), Some(10.0), Some(0.00005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "without-tools",
                    "two",
                    "failed",
                    Some((Some(30.0), Some(30.0), Some(0.00005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "two",
                    "passed",
                    Some((Some(30.0), Some(30.0), Some(0.00005))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "without-tools",
                    "three",
                    "passed",
                    Some((Some(1.0), Some(1.0), Some(0.0))),
                    "without-tools",
                    &["with-tools"],
                ),
                observation(
                    "with-tools",
                    "three",
                    "failed",
                    Some((Some(2.0), Some(2.0), Some(0.0))),
                    "without-tools",
                    &["with-tools"],
                ),
            ],
        ),
    ]
}

fn assert_matches(fixtures: &[(&str, Vec<HarnessObservation>)]) {
    let mut plain = support::TsOracle::spawn_with_environment(
        "evals-summary-oracle.ts",
        SOURCE_MODULE,
        &[("FORCE_COLOR", OsString::from("0"))],
    );
    let mut colored = support::TsOracle::spawn_with_environment(
        "evals-summary-oracle.ts",
        SOURCE_MODULE,
        &[("FORCE_COLOR", OsString::from("1"))],
    );
    for (name, observations) in fixtures {
        let request = json!({
            "id": name,
            "operation": "summarize",
            "observations": observations,
        });
        let plain_response = plain.request(&request);
        let colored_response = colored.request(&request);
        assert_eq!(plain_response["ok"], true, "{name}: {}", plain_response["error"]);
        assert_eq!(colored_response["ok"], true, "{name}: {}", colored_response["error"]);

        let report = summarize_harness_comparisons(observations);
        let report_json =
            js_json::stringify(&serde_json::to_value(&report).expect("comparison reports must serialize"));
        assert_eq!(
            plain_response["output"]["reportJson"].as_str(),
            Some(report_json.as_str()),
            "{name}: report JSON"
        );
        assert_eq!(
            plain_response["output"]["formatted"].as_str(),
            Some(format_harness_comparison_report_with_style(&report, StyleText::never()).as_str()),
            "{name}: plain report"
        );
        assert_eq!(
            colored_response["output"]["formatted"].as_str(),
            Some(format_harness_comparison_report_with_style(&report, StyleText::always()).as_str()),
            "{name}: colored report"
        );
    }
}

#[test]
fn summary_matches_the_typescript_module() {
    assert_matches(&fixtures());
}

#[test]
fn report_deserializes_from_its_own_serialization() {
    let (_, observations) = &fixtures()[0];
    let report = summarize_harness_comparisons(observations);
    let value: Value = serde_json::to_value(&report).expect("report serializes");
    let round_trip: HarnessComparisonReport = serde_json::from_value(value).expect("report deserializes");
    assert_eq!(round_trip, report);
}
