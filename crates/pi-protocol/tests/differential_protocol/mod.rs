pub(crate) mod framing;
pub(crate) mod model_cost;
pub(crate) mod oracle;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(&[
        "packages/protocol/src/framing.ts",
        "packages/protocol/src/schemas.ts",
        "node_modules/typebox/build/value/index.mjs",
    ]);
    framing::compare_all_fixtures(&mut oracle);
    model_cost::compare_all_fixtures(&mut oracle, 1000);
}
