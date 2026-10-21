use pi_js_compat::node_timers::normalize_timeout_delay;

#[test]
fn invalid_and_overflow_delays_schedule_one_millisecond() {
    for delay in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -10.0,
        -0.0,
        0.0,
        0.9,
        2_147_483_647.1,
        1e103,
    ] {
        assert_eq!(normalize_timeout_delay(delay), 1.0, "{delay}");
    }
}

#[test]
fn valid_delays_retain_fractional_milliseconds() {
    for delay in [1.0, 1.9, 2.5, 2_147_483_646.9, 2_147_483_647.0] {
        assert_eq!(normalize_timeout_delay(delay), delay);
    }
}
