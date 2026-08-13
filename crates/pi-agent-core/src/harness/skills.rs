use super::types::Skill;

/// Formats one skill invocation with optional additional instructions.
///
/// Skill fields are interpolated without XML escaping.
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String {
    let dirname = dirname_env_path(&skill.file_path);
    let skill_block = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        skill.name, skill.file_path, dirname, skill.content
    );

    match additional_instructions {
        Some(instructions) if !instructions.is_empty() => {
            format!("{skill_block}\n\n{instructions}")
        }
        _ => skill_block,
    }
}

fn dirname_env_path(path: &str) -> String {
    let normalized = path.trim_end_matches(is_path_separator);
    let Some((separator_byte_index, separator_utf16_index)) = last_separator(normalized) else {
        return "/".to_owned();
    };

    if separator_utf16_index == 2 && second_utf16_unit_is_colon(normalized) {
        return normalized[..separator_byte_index + 1].to_owned();
    }
    if separator_utf16_index == 0 {
        return "/".to_owned();
    }
    normalized[..separator_byte_index].to_owned()
}

fn last_separator(path: &str) -> Option<(usize, usize)> {
    let mut last = None;
    let mut utf16_index = 0;
    for (byte_index, character) in path.char_indices() {
        if is_path_separator(character) {
            last = Some((byte_index, utf16_index));
        }
        utf16_index += character.len_utf16();
    }
    last
}

fn is_path_separator(character: char) -> bool {
    character == '/' || character == '\\'
}

fn second_utf16_unit_is_colon(value: &str) -> bool {
    value.encode_utf16().nth(1) == Some(':' as u16)
}
