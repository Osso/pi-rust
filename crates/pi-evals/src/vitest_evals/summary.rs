//! Port of `packages/evals/src/vitest-evals/summary.ts`.
//!
//! Groups paired harness observations, computes correctness lift and paired
//! efficiency deltas, and formats the terminal report. Ordering follows the
//! TypeScript source: `localeCompare` for names and JSON-encoded group keys,
//! insertion order within a group.

use std::cmp::Ordering;

use indexmap::IndexMap;
use pi_js_compat::js_json;
use pi_js_compat::locale_compare;
use pi_js_compat::number::to_fixed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::style::{StyleText, TextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessObservationOutcome {
    Scored,
    Unscored,
    Skipped,
    Pending,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessObservationBase {
    pub eval_set: String,
    pub group_key: String,
    pub test_name: String,
    pub file: String,
    pub harness: String,
    pub baseline: String,
    pub candidates: Vec<String>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub repetition: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub total_tokens: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub total_ms: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessObservation {
    #[serde(flatten)]
    pub base: HarnessObservationBase,
    pub outcome: HarnessObservationOutcome,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedMetricSummary {
    pub total_pairs: u64,
    pub eligible_pairs: u64,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub baseline_mean: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub candidate_mean: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub mean_delta: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectnessLiftSummary {
    pub total_pairs: u64,
    pub eligible_pairs: u64,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub baseline_pass_rate: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub candidate_pass_rate: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub lift: Option<f64>,
    pub baseline_wins: u64,
    pub candidate_wins: u64,
    pub ties: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessPairComparison {
    pub baseline: String,
    pub candidate: String,
    pub correctness: CorrectnessLiftSummary,
    pub total_tokens: PairedMetricSummary,
    pub total_ms: PairedMetricSummary,
    pub estimated_cost_usd: PairedMetricSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessComparisonDiagnosticReason {
    MissingObservation,
    DuplicateObservation,
    HarnessError,
    MissingScore,
    UnscorableOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessComparisonDiagnostic {
    pub eval_set: String,
    pub group_key: String,
    pub test_name: String,
    pub file: String,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub repetition: f64,
    pub harness: String,
    pub reason: HarnessComparisonDiagnosticReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEvalSetReport {
    pub eval_set: String,
    pub comparisons: Vec<HarnessPairComparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessComparisonReport {
    pub schema_version: u64,
    pub eval_sets: Vec<HarnessEvalSetReport>,
    pub diagnostics: Vec<HarnessComparisonDiagnostic>,
}

#[derive(Debug, Clone)]
struct HarnessDescriptor {
    name: String,
    index: usize,
}

#[derive(Debug, Clone)]
struct ObservationGroup {
    eval_set: String,
    group_key: String,
    test_name: String,
    file: String,
    repetition: f64,
    observations_by_harness: IndexMap<String, Vec<HarnessObservation>>,
}

#[derive(Debug, Default)]
struct EvalSetData {
    baseline: Option<HarnessDescriptor>,
    candidates_by_name: IndexMap<String, HarnessDescriptor>,
    groups_by_key: IndexMap<String, ObservationGroup>,
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// `Number((left - right).toPrecision(15))`: round the exact difference to 15
/// significant decimal digits, then parse it back.
fn precise_difference(left: f64, right: f64) -> f64 {
    let difference = left - right;
    format!("{difference:.14e}")
        .parse::<f64>()
        .expect("15-significant-digit float formatting must parse")
}

fn group_key_string(file: &str, test_name: &str, group_key: &str) -> String {
    js_json::stringify(&Value::Array(vec![
        Value::String(file.to_owned()),
        Value::String(test_name.to_owned()),
        Value::String(group_key.to_owned()),
    ]))
}

fn group_observations(observations: &[HarnessObservation]) -> IndexMap<String, EvalSetData> {
    let mut eval_sets: IndexMap<String, EvalSetData> = IndexMap::new();
    for observation in observations {
        let eval_set = eval_sets
            .entry(observation.base.eval_set.clone())
            .or_insert_with(|| EvalSetData {
                baseline: Some(HarnessDescriptor {
                    name: observation.base.baseline.clone(),
                    index: 0,
                }),
                ..Default::default()
            });
        for (index, name) in observation.base.candidates.iter().enumerate() {
            let replace = eval_set
                .candidates_by_name
                .get(name)
                .is_none_or(|existing| index < existing.index);
            if replace {
                eval_set.candidates_by_name.insert(
                    name.clone(),
                    HarnessDescriptor {
                        name: name.clone(),
                        index,
                    },
                );
            }
        }
        let key = group_key_string(
            &observation.base.file,
            &observation.base.test_name,
            &observation.base.group_key,
        );
        let group = eval_set.groups_by_key.entry(key).or_insert_with(|| ObservationGroup {
            eval_set: observation.base.eval_set.clone(),
            group_key: observation.base.group_key.clone(),
            test_name: observation.base.test_name.clone(),
            file: observation.base.file.clone(),
            repetition: observation.base.repetition,
            observations_by_harness: IndexMap::new(),
        });
        group
            .observations_by_harness
            .entry(observation.base.harness.clone())
            .or_default()
            .push(observation.clone());
    }
    eval_sets
}

fn ordered_candidates(eval_set: &EvalSetData) -> Vec<HarnessDescriptor> {
    let mut candidates: Vec<HarnessDescriptor> = eval_set.candidates_by_name.values().cloned().collect();
    candidates.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| locale_compare(&left.name, &right.name))
    });
    candidates
}

fn ordered_harnesses(eval_set: &EvalSetData) -> Vec<HarnessDescriptor> {
    let mut harnesses = Vec::new();
    if let Some(baseline) = &eval_set.baseline {
        harnesses.push(baseline.clone());
    }
    harnesses.extend(ordered_candidates(eval_set));
    harnesses
}

fn ordered_groups(eval_set: &EvalSetData) -> Vec<ObservationGroup> {
    let mut groups: Vec<ObservationGroup> = eval_set.groups_by_key.values().cloned().collect();
    groups.sort_by(|left, right| {
        locale_compare(&left.group_key, &right.group_key).then_with(|| {
            left.repetition
                .partial_cmp(&right.repetition)
                .unwrap_or(Ordering::Equal)
        })
    });
    groups
}

fn collect_diagnostics(
    harnesses: &[HarnessDescriptor],
    groups: &[ObservationGroup],
) -> Vec<HarnessComparisonDiagnostic> {
    let mut diagnostics = Vec::new();
    for group in groups {
        for harness in harnesses {
            let empty = Vec::new();
            let observations = group.observations_by_harness.get(&harness.name).unwrap_or(&empty);
            let reason = if observations.is_empty() {
                Some(HarnessComparisonDiagnosticReason::MissingObservation)
            } else if observations.len() > 1 {
                Some(HarnessComparisonDiagnosticReason::DuplicateObservation)
            } else {
                match observations[0].outcome {
                    HarnessObservationOutcome::Errored => Some(HarnessComparisonDiagnosticReason::HarnessError),
                    HarnessObservationOutcome::Unscored => Some(HarnessComparisonDiagnosticReason::MissingScore),
                    HarnessObservationOutcome::Scored => None,
                    HarnessObservationOutcome::Skipped | HarnessObservationOutcome::Pending => {
                        Some(HarnessComparisonDiagnosticReason::UnscorableOutcome)
                    }
                }
            };
            let Some(reason) = reason else {
                continue;
            };
            diagnostics.push(HarnessComparisonDiagnostic {
                eval_set: group.eval_set.clone(),
                group_key: group.group_key.clone(),
                test_name: group.test_name.clone(),
                file: group.file.clone(),
                repetition: group.repetition,
                harness: harness.name.clone(),
                reason,
            });
        }
    }
    diagnostics
}

fn pair_observations(
    groups: &[ObservationGroup],
    baseline_harness: &str,
    candidate_harness: &str,
) -> Vec<(HarnessObservation, HarnessObservation)> {
    let mut pairs = Vec::new();
    for group in groups {
        let empty = Vec::new();
        let baseline = group.observations_by_harness.get(baseline_harness).unwrap_or(&empty);
        let candidate = group.observations_by_harness.get(candidate_harness).unwrap_or(&empty);
        if baseline.len() == 1 && candidate.len() == 1 {
            pairs.push((baseline[0].clone(), candidate[0].clone()));
        }
    }
    pairs
}

fn summarize_metric(
    pairs: &[(HarnessObservation, HarnessObservation)],
    select: impl Fn(&HarnessObservation) -> Option<f64>,
    total_pairs: u64,
) -> PairedMetricSummary {
    let mut baseline_values = Vec::new();
    let mut candidate_values = Vec::new();
    for (baseline, candidate) in pairs {
        if baseline.outcome != HarnessObservationOutcome::Scored
            || candidate.outcome != HarnessObservationOutcome::Scored
        {
            continue;
        }
        let Some(baseline_value) = select(baseline).filter(|value| value.is_finite()) else {
            continue;
        };
        let Some(candidate_value) = select(candidate).filter(|value| value.is_finite()) else {
            continue;
        };
        baseline_values.push(baseline_value);
        candidate_values.push(candidate_value);
    }

    let baseline_mean = mean(&baseline_values);
    let candidate_mean = mean(&candidate_values);
    let mean_delta = match (baseline_mean, candidate_mean) {
        (Some(baseline_mean), Some(candidate_mean)) => Some(precise_difference(candidate_mean, baseline_mean)),
        _ => None,
    };
    PairedMetricSummary {
        total_pairs,
        eligible_pairs: baseline_values.len() as u64,
        baseline_mean,
        candidate_mean,
        mean_delta,
    }
}

fn summarize_correctness(
    pairs: &[(HarnessObservation, HarnessObservation)],
    total_pairs: u64,
) -> CorrectnessLiftSummary {
    let mut eligible_pairs = 0_u64;
    let mut baseline_passes = 0_u64;
    let mut candidate_passes = 0_u64;
    let mut baseline_wins = 0_u64;
    let mut candidate_wins = 0_u64;
    let mut ties = 0_u64;

    for (baseline, candidate) in pairs {
        if baseline.outcome != HarnessObservationOutcome::Scored
            || candidate.outcome != HarnessObservationOutcome::Scored
        {
            continue;
        }
        eligible_pairs += 1;
        let baseline_passed = baseline.score.unwrap_or(f64::NAN) >= 1.0;
        let candidate_passed = candidate.score.unwrap_or(f64::NAN) >= 1.0;
        if baseline_passed {
            baseline_passes += 1;
        }
        if candidate_passed {
            candidate_passes += 1;
        }
        if baseline_passed == candidate_passed {
            ties += 1;
        } else if baseline_passed {
            baseline_wins += 1;
        } else {
            candidate_wins += 1;
        }
    }

    let pass_rate = |passes: u64| (eligible_pairs != 0).then(|| passes as f64 / eligible_pairs as f64);
    let baseline_pass_rate = pass_rate(baseline_passes);
    let candidate_pass_rate = pass_rate(candidate_passes);
    let lift = match (baseline_pass_rate, candidate_pass_rate) {
        (Some(baseline), Some(candidate)) => Some(precise_difference(candidate, baseline)),
        _ => None,
    };
    CorrectnessLiftSummary {
        total_pairs,
        eligible_pairs,
        baseline_pass_rate,
        candidate_pass_rate,
        lift,
        baseline_wins,
        candidate_wins,
        ties,
    }
}

fn compare_harnesses(
    baseline: &HarnessDescriptor,
    candidate: &HarnessDescriptor,
    groups: &[ObservationGroup],
) -> HarnessPairComparison {
    let pairs = pair_observations(groups, &baseline.name, &candidate.name);
    let total_pairs = groups.len() as u64;
    HarnessPairComparison {
        baseline: baseline.name.clone(),
        candidate: candidate.name.clone(),
        correctness: summarize_correctness(&pairs, total_pairs),
        total_tokens: summarize_metric(&pairs, |observation| observation.base.total_tokens, total_pairs),
        total_ms: summarize_metric(&pairs, |observation| observation.base.total_ms, total_pairs),
        estimated_cost_usd: summarize_metric(&pairs, |observation| observation.base.estimated_cost_usd, total_pairs),
    }
}

pub fn summarize_harness_comparisons(observations: &[HarnessObservation]) -> HarnessComparisonReport {
    let mut eval_sets = Vec::new();
    let mut diagnostics = Vec::new();
    let grouped = group_observations(observations);
    let mut ordered_eval_sets: Vec<(&String, &EvalSetData)> = grouped.iter().collect();
    ordered_eval_sets.sort_by(|(left, _), (right, _)| locale_compare(left, right));
    for (eval_set_name, data) in ordered_eval_sets {
        let harnesses = ordered_harnesses(data);
        let candidates = ordered_candidates(data);
        let groups = ordered_groups(data);
        let comparisons = candidates
            .iter()
            .map(|candidate| {
                let baseline = data
                    .baseline
                    .as_ref()
                    .expect("every eval set has a baseline descriptor");
                compare_harnesses(baseline, candidate, &groups)
            })
            .collect();
        eval_sets.push(HarnessEvalSetReport {
            eval_set: eval_set_name.clone(),
            comparisons,
        });
        diagnostics.extend(collect_diagnostics(&harnesses, &groups));
    }

    diagnostics.sort_by(|left, right| {
        locale_compare(&left.eval_set, &right.eval_set)
            .then_with(|| locale_compare(&left.file, &right.file))
            .then_with(|| locale_compare(&left.group_key, &right.group_key))
            .then_with(|| {
                left.repetition
                    .partial_cmp(&right.repetition)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| locale_compare(&left.harness, &right.harness))
    });

    HarnessComparisonReport {
        schema_version: 1,
        eval_sets,
        diagnostics,
    }
}

fn format_percentage(value: Option<f64>) -> String {
    match value {
        None => "unavailable".to_owned(),
        Some(value) => format!("{}%", to_fixed(value * 100.0, 1)),
    }
}

fn format_signed(value: f64, fraction_digits: u32) -> String {
    let sign = if value >= 0.0 { "+" } else { "" };
    format!("{sign}{}", to_fixed(value, fraction_digits))
}

fn format_coverage(eligible_pairs: u64, total_pairs: u64, style: StyleText) -> String {
    style.style(TextStyle::Gray, &format!("({eligible_pairs}/{total_pairs} pairs)"))
}

fn format_report_line(label: &str, value: &str, style: StyleText) -> String {
    format!("    {}  {value}", style.style(TextStyle::Gray, &format!("{label:>9}")))
}

fn color_delta(value: f64, formatted: &str, positive_is_better: bool, style: StyleText) -> String {
    if value == 0.0 {
        return style.style(TextStyle::Gray, formatted);
    }
    let improved = if positive_is_better { value > 0.0 } else { value < 0.0 };
    style.style(if improved { TextStyle::Green } else { TextStyle::Red }, formatted)
}

fn format_metric(
    label: &str,
    metric: &PairedMetricSummary,
    format_value: impl Fn(f64) -> String,
    format_delta: impl Fn(f64) -> String,
    comparison_pairs: u64,
    style: StyleText,
) -> String {
    let coverage = if metric.eligible_pairs == 0 || metric.eligible_pairs == comparison_pairs {
        String::new()
    } else {
        format!(" {}", format_coverage(metric.eligible_pairs, metric.total_pairs, style))
    };
    let (Some(baseline_mean), Some(candidate_mean), Some(mean_delta)) =
        (metric.baseline_mean, metric.candidate_mean, metric.mean_delta)
    else {
        return format_report_line(
            label,
            &format!("{}{coverage}", style.style(TextStyle::Yellow, "unavailable")),
            style,
        );
    };
    let delta = color_delta(mean_delta, &format_delta(mean_delta), false, style);
    let values = style.style(
        TextStyle::Gray,
        &format!(
            "(candidate {}, baseline {})",
            format_value(candidate_mean),
            format_value(baseline_mean)
        ),
    );
    format_report_line(label, &format!("{delta} {values}{coverage}"), style)
}

pub fn format_harness_comparison_report(report: &HarnessComparisonReport) -> String {
    format_harness_comparison_report_with_style(report, StyleText::detect())
}

pub fn format_harness_comparison_report_with_style(report: &HarnessComparisonReport, style: StyleText) -> String {
    if report.eval_sets.iter().all(|eval_set| eval_set.comparisons.is_empty()) {
        return String::new();
    }
    let mut lines = vec![style.style(TextStyle::Bold, "Eval Comparisons")];
    for eval_set in &report.eval_sets {
        lines.push(format!("  {}", eval_set.eval_set));
        for (index, comparison) in eval_set.comparisons.iter().enumerate() {
            if index > 0 {
                lines.push(String::new());
            }
            let correctness = &comparison.correctness;
            lines.push(format_report_line("Baseline", &comparison.baseline, style));
            lines.push(format_report_line(
                "Candidate",
                &format!(
                    "{} {}",
                    comparison.candidate,
                    format_coverage(correctness.eligible_pairs, correctness.total_pairs, style)
                ),
                style,
            ));
            match correctness.lift {
                None => lines.push(format_report_line(
                    "Pass rate",
                    &style.style(TextStyle::Yellow, "unavailable"),
                    style,
                )),
                Some(lift) => {
                    let percentage_points = lift * 100.0;
                    let delta = color_delta(
                        percentage_points,
                        &format!("{} pp", format_signed(percentage_points, 1)),
                        true,
                        style,
                    );
                    let values = style.style(
                        TextStyle::Gray,
                        &format!(
                            "(candidate {}, baseline {})",
                            format_percentage(correctness.candidate_pass_rate),
                            format_percentage(correctness.baseline_pass_rate)
                        ),
                    );
                    lines.push(format_report_line("Pass rate", &format!("{delta} {values}"), style));
                }
            }
            lines.push(format_metric(
                "Tokens",
                &comparison.total_tokens,
                |value| to_fixed(value, 1),
                |value| format_signed(value, 1),
                correctness.eligible_pairs,
                style,
            ));
            lines.push(format_metric(
                "Latency",
                &comparison.total_ms,
                |value| format!("{}ms", to_fixed(value, 1)),
                |value| format!("{}ms", format_signed(value, 1)),
                correctness.eligible_pairs,
                style,
            ));
            lines.push(format_metric(
                "Est. cost",
                &comparison.estimated_cost_usd,
                |value| format!("${}", to_fixed(value, 4)),
                |value| {
                    let sign = if value >= 0.0 { "+" } else { "-" };
                    format!("{sign}${}", to_fixed(value.abs(), 4))
                },
                correctness.eligible_pairs,
                style,
            ));
        }
    }
    if !report.diagnostics.is_empty() {
        lines.push(format!(
            "  {}",
            style.style(TextStyle::Yellow, "Incomplete observations")
        ));
        for diagnostic in &report.diagnostics {
            lines.push(format!(
                "    {}: {}/{} repetition {}, harness {}",
                diagnostic_reason_label(diagnostic.reason),
                diagnostic.file,
                diagnostic.test_name,
                js_json::stringify(&json!(diagnostic.repetition)),
                diagnostic.harness
            ));
        }
    }
    lines.join("\n")
}

fn diagnostic_reason_label(reason: HarnessComparisonDiagnosticReason) -> &'static str {
    match reason {
        HarnessComparisonDiagnosticReason::MissingObservation => "missing-observation",
        HarnessComparisonDiagnosticReason::DuplicateObservation => "duplicate-observation",
        HarnessComparisonDiagnosticReason::HarnessError => "harness-error",
        HarnessComparisonDiagnosticReason::MissingScore => "missing-score",
        HarnessComparisonDiagnosticReason::UnscorableOutcome => "unscorable-outcome",
    }
}
