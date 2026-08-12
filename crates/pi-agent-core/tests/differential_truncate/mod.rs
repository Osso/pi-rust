mod format_size;
mod oracle;
mod support;
mod tail_corpus;
mod targeted;
mod truncate_line;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("packages/agent/src/harness/utils/truncate.ts");
    format_size::compare_all_fixtures(&mut oracle);
    targeted::compare_all_fixtures(&mut oracle);
    truncate_line::compare_all_fixtures(&mut oracle);
    targeted::compare_utf8_fallback(&mut oracle);
    tail_corpus::compare_corpus(&mut oracle);
}
