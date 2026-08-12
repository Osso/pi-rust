pub(crate) mod framing;
pub(crate) mod model_cost;
pub(crate) mod model_metadata;
pub(crate) mod oracle;
pub(crate) mod schema_leaves;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(&[
        "packages/protocol/src/framing.ts",
        "packages/protocol/src/schemas.ts",
        "node_modules/typebox/build/value/index.mjs",
    ]);
    framing::compare_all_fixtures(&mut oracle);
    model_cost::compare_all_fixtures(&mut oracle, 1000);
    schema_leaves::compare_all_fixtures(&mut oracle);
    model_metadata::compare_all_fixtures(&mut oracle);
}
