use pi_protocol::{ModelCost, ModelCostError};

#[test]
fn accepts_integer_fractional_zero_and_negative_zero_costs() {
    let cost = ModelCost::new(1000.0, 110.5, 0.0, -0.0).expect("valid costs must construct");

    assert_eq!(cost.input(), 1000.0);
    assert_eq!(cost.output(), 110.5);
    assert_eq!(cost.cache_read(), 0.0);
    assert_eq!(cost.cache_write().to_bits(), (-0.0f64).to_bits());
}

#[test]
fn accepts_all_finite_non_negative_f64_values() {
    let cost = ModelCost::new(f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), 0.0)
        .expect("all finite non-negative f64 values must construct");

    assert_eq!(cost.input(), f64::MAX);
    assert_eq!(cost.output(), f64::MIN_POSITIVE);
    assert_eq!(cost.cache_read(), f64::from_bits(1));
    assert_eq!(cost.cache_write(), 0.0);
}

#[test]
fn rejects_negative_fields() {
    assert_eq!(ModelCost::new(-1.0, 0.0, 0.0, 0.0), Err(ModelCostError::InvalidInput));
    assert_eq!(ModelCost::new(0.0, -1.0, 0.0, 0.0), Err(ModelCostError::InvalidOutput));
    assert_eq!(
        ModelCost::new(0.0, 0.0, -f64::MIN_POSITIVE, 0.0),
        Err(ModelCostError::InvalidCacheRead)
    );
    assert_eq!(
        ModelCost::new(0.0, 0.0, 0.0, -1.0),
        Err(ModelCostError::InvalidCacheWrite)
    );
}

#[test]
fn rejects_non_finite_fields() {
    assert_eq!(
        ModelCost::new(f64::NAN, 0.0, 0.0, 0.0),
        Err(ModelCostError::InvalidInput)
    );
    assert_eq!(
        ModelCost::new(0.0, f64::INFINITY, 0.0, 0.0),
        Err(ModelCostError::InvalidOutput)
    );
    assert_eq!(
        ModelCost::new(0.0, 0.0, f64::NEG_INFINITY, 0.0),
        Err(ModelCostError::InvalidCacheRead)
    );
    assert_eq!(
        ModelCost::new(0.0, 0.0, 0.0, f64::NAN),
        Err(ModelCostError::InvalidCacheWrite)
    );
}

#[test]
fn reports_the_first_invalid_source_field() {
    assert_eq!(
        ModelCost::new(0.0, -1.0, f64::NAN, f64::INFINITY),
        Err(ModelCostError::InvalidOutput)
    );
}

#[test]
fn validation_errors_name_the_invalid_field() {
    assert_eq!(
        ModelCostError::InvalidCacheRead.to_string(),
        "Model cost field cacheRead must be a finite number greater than or equal to 0"
    );
}
