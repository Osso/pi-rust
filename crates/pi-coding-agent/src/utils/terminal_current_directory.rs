use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

/// OSC 7 announces the logical current directory to terminals so actions such
/// as opening a new tab can inherit the application-level cwd.
pub fn format_terminal_current_directory_sequence(cwd: &str) -> String {
    let path = resolve_absolute(cwd);
    format!(
        "\x1b]7;file://{}{}\x1b\\",
        gethostname::gethostname().to_string_lossy(),
        encode_file_url_path(&path)
    )
}

fn resolve_absolute(path: &str) -> String {
    let keeps_trailing_separator = path.ends_with('/');
    let candidate = Path::new(path);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(candidate)
    };
    let mut normalized = normalize_lexically(&absolute).to_string_lossy().into_owned();
    if keeps_trailing_separator && normalized != "/" && !normalized.ends_with('/') {
        normalized.push('/');
    }
    normalized
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

fn encode_file_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if is_unreserved_file_url_byte(byte) {
            encoded.push(char::from(byte));
            continue;
        }
        let _ = write!(encoded, "%{byte:02X}");
    }
    encoded
}

fn is_unreserved_file_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'/'
                | b':'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}
