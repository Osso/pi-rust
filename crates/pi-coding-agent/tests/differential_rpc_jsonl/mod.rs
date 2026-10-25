mod decoder;
use crate::shared_oracle as oracle;
mod serialization;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("rpc-jsonl-oracle.ts", "packages/coding-agent/src/modes/rpc/jsonl.ts");
    serialization::compare_all_fixtures(&mut oracle);
    decoder::compare_all_fixtures(&mut oracle);
}
