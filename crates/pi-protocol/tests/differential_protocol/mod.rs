pub(crate) mod framing;
pub(crate) mod oracle;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("packages/protocol/src/framing.ts");
    framing::compare_all_fixtures(&mut oracle);
}
