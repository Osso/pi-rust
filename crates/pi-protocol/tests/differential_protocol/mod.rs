pub(crate) mod content_leaves;
pub(crate) mod framing;
pub(crate) mod model_cost;
pub(crate) mod model_metadata;
pub(crate) mod oracle;
pub(crate) mod schema_leaves;
pub(crate) mod session_metadata;
pub(crate) mod usage;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(&[
        "packages/protocol/src/framing.ts",
        "packages/protocol/src/schemas.ts",
        "node_modules/typebox/build/value/index.mjs",
    ]);
    content_leaves::compare_all_fixtures(&mut oracle);
    framing::compare_all_fixtures(&mut oracle);
    model_cost::compare_all_fixtures(&mut oracle, 1000);
    schema_leaves::compare_all_fixtures(&mut oracle);
    session_metadata::compare_all_fixtures(&mut oracle);
    model_metadata::compare_all_fixtures(&mut oracle);
    usage::compare_all_fixtures(&mut oracle);
}
