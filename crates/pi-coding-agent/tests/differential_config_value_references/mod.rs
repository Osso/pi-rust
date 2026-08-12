mod fixtures;
#[path = "../support/oracle.rs"]
mod oracle;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "config-value-reference-oracle.ts",
        "packages/coding-agent/src/core/resolve-config-value.ts",
    );

    for fixture in fixtures::all() {
        let expected = oracle.request(&fixture.request);
        assert_eq!(
            expected, fixture.expected,
            "config-value reference mismatch: {}",
            fixture.name
        );
    }
}
