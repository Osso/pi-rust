use super::types::Skill;

const PREAMBLE: [&str; 5] = [
    "The following skills provide specialized instructions for specific tasks.",
    "Read the full skill file when the task matches its description.",
    "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.",
    "",
    "<available_skills>",
];

/// Formats visible skills for the system prompt.
///
/// Metadata fields are XML-escaped; skill content is intentionally omitted.
pub fn format_skills_for_system_prompt(skills: &[Skill]) -> String {
    let visible_skills: Vec<&Skill> = skills
        .iter()
        .filter(|skill| skill.disable_model_invocation != Some(true))
        .collect();
    if visible_skills.is_empty() {
        return String::new();
    }

    let mut lines = PREAMBLE.map(str::to_owned).to_vec();
    for skill in visible_skills {
        lines.push("  <skill>".to_owned());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!("    <location>{}</location>", escape_xml(&skill.file_path)));
        lines.push("  </skill>".to_owned());
    }
    lines.push("</available_skills>".to_owned());
    lines.join("\n")
}

fn escape_xml(value: &str) -> String {
    let escaped_ampersands = value.replace('&', "&amp;");
    let escaped_less_than = escaped_ampersands.replace('<', "&lt;");
    let escaped_greater_than = escaped_less_than.replace('>', "&gt;");
    let escaped_double_quotes = escaped_greater_than.replace('"', "&quot;");
    escaped_double_quotes.replace('\'', "&apos;")
}
