mod agent_tool_execution_context;
mod bash_execution_message;
mod branch_summary_message;
mod compaction_details;
mod compaction_settings;
mod context_usage_estimate;
mod cut_point_result;
mod edit_line_endings;
mod errors;
mod file_info;
mod file_operations;
mod format_size;
mod image;
mod oracle;
mod prompt_template_diagnostic;
mod prompt_templates;
mod queue_mode;
mod resources;
mod sanitize;
mod skill_formatting;
mod summarization_system_prompt;
mod summary_markers;
mod support;
mod tail_corpus;
mod targeted;
mod thinking_level;
mod tool_execution_mode;
mod truncate_line;

fn spawn_oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(&[
        "packages/agent/src/harness/utils/truncate.ts",
        "packages/agent/src/harness/utils/shell-output.ts",
        "packages/agent/src/index.ts",
        "packages/agent/src/harness/prompt-templates.ts",
        "packages/agent/src/harness/skills.ts",
        "packages/agent/src/harness/system-prompt.ts",
        "packages/agent/src/harness/tools/image.ts",
        "packages/agent/src/harness/tools/edit-diff.ts",
        "packages/agent/src/harness/compaction/utils.ts",
        "packages/agent/src/harness/compaction/compaction.ts",
    ])
}

pub(crate) fn run() {
    let mut oracle = spawn_oracle();
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
    file_operations::compare_all_fixtures(&mut oracle);
    summary_markers::compare(&mut oracle);
    bash_execution_message::compare_all_fixtures(&mut oracle);
    branch_summary_message::compare_all_fixtures(&mut oracle);
    compaction_settings::compare_all_fixtures(&mut oracle);
    prompt_template_diagnostic::compare_all_fixtures(&mut oracle);
    summarization_system_prompt::compare(&mut oracle);
    cut_point_result::compare_all_fixtures(&mut oracle);
    context_usage_estimate::compare_all_fixtures(&mut oracle);
    compaction_details::compare_all_fixtures(&mut oracle);
    queue_mode::compare(&mut oracle);
    tool_execution_mode::compare(&mut oracle);
    thinking_level::compare(&mut oracle);
    agent_tool_execution_context::compare_all_fixtures(&mut oracle);
}
