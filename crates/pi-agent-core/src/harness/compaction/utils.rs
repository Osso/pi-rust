/// Formats read-only and modified file lists as summary metadata sections.
pub fn format_file_operations(read_files: &[String], modified_files: &[String]) -> String {
    let mut sections = Vec::new();

    if !read_files.is_empty() {
        sections.push(format_section("read-files", read_files));
    }
    if !modified_files.is_empty() {
        sections.push(format_section("modified-files", modified_files));
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", sections.join("\n\n"))
    }
}

fn format_section(tag: &str, entries: &[String]) -> String {
    format!("<{tag}>\n{}\n</{tag}>", entries.join("\n"))
}
