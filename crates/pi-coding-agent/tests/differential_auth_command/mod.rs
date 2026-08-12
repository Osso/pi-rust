mod fixtures;
#[path = "../oracle.rs"]
mod oracle;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "auth-command-oracle.ts",
        "packages/coding-agent/src/cli/auth-command.ts",
    );

    for fixture in fixtures::all() {
        let expected = oracle.request(&fixture.request);
        assert_eq!(expected, fixture.expected, "auth command mismatch: {}", fixture.name);
    }
}
