mod edit_line_endings;
mod errors;
mod file_info;
mod format_size;
mod image;
mod oracle;
mod prompt_templates;
mod resources;
mod sanitize;
mod skill_formatting;
mod support;
mod tail_corpus;
mod targeted;
mod truncate_line;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(&[
        "packages/agent/src/harness/utils/truncate.ts",
        "packages/agent/src/harness/utils/shell-output.ts",
        "packages/agent/src/index.ts",
        "packages/agent/src/harness/prompt-templates.ts",
        "packages/agent/src/harness/skills.ts",
        "packages/agent/src/harness/system-prompt.ts",
        "packages/agent/src/harness/tools/image.ts",
        "packages/agent/src/harness/tools/edit-diff.ts",
    ]);
    format_size::compare_all_fixtures(&mut oracle);
    sanitize::compare_all_fixtures(&mut oracle);
    targeted::compare_all_fixtures(&mut oracle);
    truncate_line::compare_all_fixtures(&mut oracle);
    targeted::compare_utf8_fallback(&mut oracle);
    tail_corpus::compare_corpus(&mut oracle);
    errors::compare_existing_fixtures(&mut oracle);
    file_info::compare_all_fixtures(&mut oracle);
    resources::compare_all_fixtures(&mut oracle);
    errors::compare_compaction_fixtures(&mut oracle);
    prompt_templates::compare_all_fixtures(&mut oracle);
    skill_formatting::compare_all_fixtures(&mut oracle);
    image::compare_all_fixtures(&mut oracle);
    edit_line_endings::compare_all_fixtures(&mut oracle);
}
