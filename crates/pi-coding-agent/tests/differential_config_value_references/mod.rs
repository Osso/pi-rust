mod fixtures;
use crate::shared_oracle as oracle;
mod resolve;
mod resolve_probe;
mod status;

#[test]
#[ignore = "child probe entry point"]
fn resolve_process_probe_child() {
    resolve_probe::run_child();
}

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

    status::run(&mut oracle);
    resolve::run(&mut oracle);

    #[cfg(unix)]
    status::run_non_unicode_status();
}
