const INTERLINEAR_ANNOTATION_START: u32 = 0xfff9;
const INTERLINEAR_ANNOTATION_END: u32 = 0xfffb;

/// Removes binary control characters that should not appear in captured shell text.
pub fn sanitize_binary_output(input: &str) -> String {
    input.chars().filter(|character| keep_code_point(*character)).collect()
}

fn keep_code_point(character: char) -> bool {
    let code_point = u32::from(character);
    matches!(code_point, 0x09 | 0x0a | 0x0d)
        || !(code_point <= 0x1f || (INTERLINEAR_ANNOTATION_START..=INTERLINEAR_ANNOTATION_END).contains(&code_point))
}
