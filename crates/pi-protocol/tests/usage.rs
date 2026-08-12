use pi_protocol::{Usage, UsageCost, UsageCostError, UsageInput};

fn usage_input(input: f64, output: f64, cache_read: f64, cache_write: f64, total: f64) -> UsageInput {
    UsageInput {
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        total_tokens: 5,
        reasoning: Some(6),
        cost: UsageCost::new(input, output, cache_read, cache_write, total).expect("fixture cost must be valid"),
    }
}

#[test]
fn usage_cost_accepts_finite_non_negative_values_and_preserves_bits() {
    let cost =
        UsageCost::new(1000.0, 110.5, 0.0, -0.0, f64::from_bits(1)).expect("finite non-negative costs must construct");

    assert_eq!(cost.input(), 1000.0);
    assert_eq!(cost.output(), 110.5);
    assert_eq!(cost.cache_read(), 0.0);
    assert_eq!(cost.cache_write().to_bits(), (-0.0f64).to_bits());
    assert_eq!(cost.total().to_bits(), f64::from_bits(1).to_bits());

    let extremes = UsageCost::new(f64::MAX, f64::MIN_POSITIVE, 0.0, -0.0, f64::MAX)
        .expect("finite non-negative extremes must construct");
    assert_eq!(extremes.input(), f64::MAX);
    assert_eq!(extremes.output(), f64::MIN_POSITIVE);
}

#[test]
fn usage_cost_rejects_negative_fields() {
    assert_eq!(
        UsageCost::new(-1.0, 0.0, 0.0, 0.0, 0.0),
        Err(UsageCostError::InvalidInput)
    );
    assert_eq!(
        UsageCost::new(0.0, -1.0, 0.0, 0.0, 0.0),
        Err(UsageCostError::InvalidOutput)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, -f64::MIN_POSITIVE, 0.0, 0.0),
        Err(UsageCostError::InvalidCacheRead)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, 0.0, -1.0, 0.0),
        Err(UsageCostError::InvalidCacheWrite)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, 0.0, 0.0, -1.0),
        Err(UsageCostError::InvalidTotal)
    );
}

#[test]
fn usage_cost_rejects_non_finite_fields() {
    assert_eq!(
        UsageCost::new(f64::NAN, 0.0, 0.0, 0.0, 0.0),
        Err(UsageCostError::InvalidInput)
    );
    assert_eq!(
        UsageCost::new(0.0, f64::INFINITY, 0.0, 0.0, 0.0),
        Err(UsageCostError::InvalidOutput)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, f64::NEG_INFINITY, 0.0, 0.0),
        Err(UsageCostError::InvalidCacheRead)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, 0.0, f64::NAN, 0.0),
        Err(UsageCostError::InvalidCacheWrite)
    );
    assert_eq!(
        UsageCost::new(0.0, 0.0, 0.0, 0.0, f64::INFINITY),
        Err(UsageCostError::InvalidTotal)
    );
}

#[test]
fn usage_cost_reports_first_invalid_field_in_source_order() {
    assert_eq!(
        UsageCost::new(0.0, -1.0, f64::NAN, f64::INFINITY, -1.0),
        Err(UsageCostError::InvalidOutput)
    );
}

#[test]
fn usage_cost_errors_name_the_invalid_field() {
    assert_eq!(
        UsageCostError::InvalidTotal.to_string(),
        "Usage cost field total must be a finite number greater than or equal to 0"
    );
}

#[test]
fn usage_preserves_token_counts_reasoning_and_cost_without_recomputing_total() {
    let usage = Usage::new(usage_input(1000.0, 110.5, 0.0, -0.0, 42.25));

    assert_eq!(usage.input(), 1);
    assert_eq!(usage.output(), 2);
    assert_eq!(usage.cache_read(), 3);
    assert_eq!(usage.cache_write(), 4);
    assert_eq!(usage.total_tokens(), 5);
    assert_eq!(usage.reasoning(), Some(6));
    assert_eq!(usage.cost().input(), 1000.0);
    assert_eq!(usage.cost().output(), 110.5);
    assert_eq!(usage.cost().cache_write().to_bits(), (-0.0f64).to_bits());
    assert_eq!(usage.cost().total(), 42.25);
}

#[test]
fn usage_preserves_none_separately_from_some_zero_reasoning() {
    let mut input = usage_input(0.0, 0.0, 0.0, 0.0, 0.0);
    input.reasoning = None;
    assert_eq!(Usage::new(input).reasoning(), None);

    let mut input = usage_input(0.0, 0.0, 0.0, 0.0, 0.0);
    input.reasoning = Some(0);
    assert_eq!(Usage::new(input).reasoning(), Some(0));
}

#[test]
fn usage_accepts_zero_and_maximum_token_counts() {
    let mut input = usage_input(0.0, 0.0, 0.0, 0.0, 0.0);
    input.input = 0;
    input.output = 0;
    input.cache_read = 0;
    input.cache_write = 0;
    input.total_tokens = 0;
    input.reasoning = Some(0);
    let zero = Usage::new(input);
    assert_eq!(zero.input(), 0);
    assert_eq!(zero.reasoning(), Some(0));

    let mut input = usage_input(0.0, 0.0, 0.0, 0.0, 0.0);
    input.input = u64::MAX;
    input.output = u64::MAX;
    input.cache_read = u64::MAX;
    input.cache_write = u64::MAX;
    input.total_tokens = u64::MAX;
    input.reasoning = Some(u64::MAX);
    let maximum = Usage::new(input);
    assert_eq!(maximum.input(), u64::MAX);
    assert_eq!(maximum.output(), u64::MAX);
    assert_eq!(maximum.cache_read(), u64::MAX);
    assert_eq!(maximum.cache_write(), u64::MAX);
    assert_eq!(maximum.total_tokens(), u64::MAX);
    assert_eq!(maximum.reasoning(), Some(u64::MAX));
}
