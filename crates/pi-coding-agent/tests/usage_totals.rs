use pi_ai::types::{Usage, UsageCost};
use pi_coding_agent::core::usage_totals::{UsageTotals, add_usage_to_totals, create_usage_totals};

#[test]
fn creates_and_accumulates_usage_totals() {
    let mut totals = create_usage_totals();
    assert_eq!(totals, UsageTotals::default());
    add_usage_to_totals(
        &mut totals,
        &Usage {
            input: 1.0,
            output: 2.0,
            cache_read: 3.0,
            cache_write: 4.0,
            cost: UsageCost {
                total: 5.0,
                ..UsageCost::default()
            },
            ..Usage::default()
        },
    );
    assert_eq!(totals.input, 1.0);
    assert_eq!(totals.output, 2.0);
    assert_eq!(totals.cache_read, 3.0);
    assert_eq!(totals.cache_write, 4.0);
    assert_eq!(totals.cost, 5.0);
}
