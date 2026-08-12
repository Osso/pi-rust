mod errors;
mod format_size;
mod oracle;
mod sanitize;
mod support;
mod tail_corpus;
mod targeted;
mod truncate_line;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(&[
        "packages/agent/src/harness/utils/truncate.ts",
        "packages/agent/src/harness/utils/shell-output.ts",
        "packages/agent/src/harness/types.ts",
    ]);
    format_size::compare_all_fixtures(&mut oracle);
    sanitize::compare_all_fixtures(&mut oracle);
    targeted::compare_all_fixtures(&mut oracle);
    truncate_line::compare_all_fixtures(&mut oracle);
    targeted::compare_utf8_fallback(&mut oracle);
    tail_corpus::compare_corpus(&mut oracle);
    errors::compare_all_fixtures(&mut oracle);
}
