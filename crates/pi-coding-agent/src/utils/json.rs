/// Strip `//` line comments and trailing commas from JSON while leaving string
/// literals untouched.
pub fn strip_json_comments(input: &str) -> String {
    let characters = input.chars().collect::<Vec<_>>();
    let without_comments = remove_line_comments(&characters);
    remove_trailing_commas(&without_comments)
}

fn remove_line_comments(characters: &[char]) -> Vec<char> {
    let mut output = Vec::with_capacity(characters.len());
    let mut index = 0;

    while index < characters.len() {
        if characters[index] == '"' {
            if let Some(end) = string_literal_end(characters, index) {
                output.extend_from_slice(&characters[index..end]);
                index = end;
                continue;
            }
        } else if characters[index] == '/' && characters.get(index + 1) == Some(&'/') {
            index += 2;
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
            continue;
        }

        output.push(characters[index]);
        index += 1;
    }

    output
}

fn remove_trailing_commas(characters: &[char]) -> String {
    let mut output = String::with_capacity(characters.len());
    let mut index = 0;

    while index < characters.len() {
        if characters[index] == '"' {
            if let Some(end) = string_literal_end(characters, index) {
                output.extend(&characters[index..end]);
                index = end;
                continue;
            }
        } else if characters[index] == ',' && is_trailing_comma(characters, index + 1) {
            index += 1;
            continue;
        }

        output.push(characters[index]);
        index += 1;
    }

    output
}

fn string_literal_end(characters: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while index < characters.len() {
        match characters[index] {
            '\\' => {
                let escaped = characters.get(index + 1)?;
                if matches!(escaped, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    return None;
                }
                index += 2;
            }
            '"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

fn is_trailing_comma(characters: &[char], mut index: usize) -> bool {
    while let Some(character) = characters.get(index) {
        if is_javascript_whitespace(*character) {
            index += 1;
            continue;
        }
        return *character == '}' || *character == ']';
    }
    false
}

fn is_javascript_whitespace(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}
