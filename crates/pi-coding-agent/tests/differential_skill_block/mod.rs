mod fixtures;
use crate::shared_oracle as oracle;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("skill-block-oracle.ts", "packages/coding-agent/src/index.ts");
    fixtures::compare_all(&mut oracle);
}
