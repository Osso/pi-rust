use std::path::{Component, Path, PathBuf};

use percent_encoding::percent_decode_str;

/// Options for path normalization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathInputOptions {
    /// Trim leading and trailing whitespace before normalization.
    pub trim: bool,
    /// Expand a leading `~` to a home directory. Defaults to true.
    pub expand_tilde: Option<bool>,
    /// Home directory used for `~` expansion. Defaults to the process home.
    pub home_dir: Option<String>,
    /// Strip a leading `@`, used for CLI `@file` paths.
    pub strip_at_prefix: bool,
    /// Normalize Unicode space variants to regular spaces.
    pub normalize_unicode_spaces: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    message: String,
}

impl PathError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PathError {}

/// Resolve a path to its canonical (real) form, following symlinks.
///
/// Falls back to the raw path when resolution fails, so callers never fail on
/// missing filesystem entries.
pub fn canonicalize_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

/// Return a stable file revision string, or `None` when metadata is unavailable.
pub fn get_file_revision(path: &str) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = std::fs::metadata(path).ok()?;
        let mtime_ns = i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec());
        let ctime_ns = i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec());
        Some(format!(
            "{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            mtime_ns,
            ctime_ns
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// True when the value is not a package source or remote URL protocol.
pub fn is_local_path(value: &str) -> bool {
    let trimmed = value.trim();
    !["npm:", "git:", "github:", "http:", "https:", "ssh:"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Convert Git Bash, MSYS, Cygwin, and WSL drive paths to native Windows form.
pub fn normalize_windows_shell_path(file_path: &str) -> String {
    if !file_path.starts_with('/') || file_path.starts_with("//") || file_path.contains('\\') {
        return file_path.to_owned();
    }

    let Some(rest) = file_path.strip_prefix('/') else {
        return file_path.to_owned();
    };
    let rest = strip_ascii_prefix(rest, "mnt/")
        .or_else(|| strip_ascii_prefix(rest, "cygdrive/"))
        .unwrap_or(rest);

    let mut characters = rest.chars();
    let Some(drive) = characters.next() else {
        return file_path.to_owned();
    };
    if !drive.is_ascii_alphabetic() {
        return file_path.to_owned();
    }

    let remainder = characters.as_str();
    let suffix = match remainder.strip_prefix('/') {
        Some(value) => value.replace('/', "\\"),
        None if remainder.is_empty() => String::new(),
        None => return file_path.to_owned(),
    };

    format!("{}:\\{suffix}", drive.to_ascii_uppercase())
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

/// Normalize a path input without resolving it against a base directory.
pub fn normalize_path(input: &str, options: &PathInputOptions) -> Result<String, PathError> {
    let normalized = normalize_input(input, options);
    if let Some(expanded) = expand_home_directory(&normalized, options) {
        return Ok(expanded);
    }
    if normalized.starts_with("file://") {
        return file_url_to_path(&normalized);
    }
    Ok(normalized)
}

fn normalize_input(input: &str, options: &PathInputOptions) -> String {
    let mut normalized = if options.trim {
        input.trim().to_owned()
    } else {
        input.to_owned()
    };
    if options.normalize_unicode_spaces {
        normalized = replace_unicode_spaces(&normalized);
    }
    if options.strip_at_prefix {
        normalized = strip_at_prefix(&normalized);
    }
    if cfg!(windows) {
        normalized = normalize_windows_shell_path(&normalized);
    }
    normalized
}

fn replace_unicode_spaces(text: &str) -> String {
    text.chars()
        .map(|character| if is_unicode_space(character) { ' ' } else { character })
        .collect()
}

fn strip_at_prefix(text: &str) -> String {
    text.strip_prefix('@').unwrap_or(text).to_owned()
}

fn expand_home_directory(normalized: &str, options: &PathInputOptions) -> Option<String> {
    if !options.expand_tilde.unwrap_or(true) {
        return None;
    }
    let home = options.home_dir.clone().unwrap_or_else(home_directory);
    if normalized == "~" {
        return Some(home);
    }
    let tilde_prefix = if cfg!(windows) { "~\\" } else { "~/" };
    let remainder = normalized.strip_prefix(tilde_prefix)?;
    Some(node_join(&home, remainder))
}

/// Resolve an input path against a base directory.
pub fn resolve_path(input: &str, base_dir: &str, options: &PathInputOptions) -> Result<String, PathError> {
    let normalized = normalize_path(input, options)?;
    let normalized_base_dir = normalize_path(base_dir, &PathInputOptions::default())?;

    if Path::new(&normalized).is_absolute() {
        return Ok(lexical_normalize(&PathBuf::from(normalized)));
    }
    Ok(node_resolve(&normalized_base_dir, &normalized))
}

/// Return a path relative to the cwd when it stays inside the cwd.
pub fn get_cwd_relative_path(file_path: &str, cwd: &str) -> Result<Option<String>, PathError> {
    let resolved_cwd = resolve_from_process_cwd(cwd)?;
    let resolved_path = resolve_path(file_path, &resolved_cwd, &PathInputOptions::default())?;
    let relative_path = relative_path(&resolved_cwd, &resolved_path);

    let is_inside_cwd = relative_path.is_empty()
        || (relative_path != ".."
            && !relative_path.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR))
            && !Path::new(&relative_path).is_absolute());

    Ok(is_inside_cwd.then(|| {
        if relative_path.is_empty() {
            ".".to_owned()
        } else {
            relative_path
        }
    }))
}

/// Format a path relative to the cwd when possible, otherwise as an absolute
/// path with forward slashes.
pub fn format_path_relative_to_cwd_or_absolute(file_path: &str, cwd: &str) -> Result<String, PathError> {
    let absolute_path = resolve_path(file_path, cwd, &PathInputOptions::default())?;
    let relative = get_cwd_relative_path(&absolute_path, cwd)?;
    let text = relative.unwrap_or(absolute_path);
    Ok(if cfg!(windows) { text.replace('\\', "/") } else { text })
}

/// Mark a path as ignored by cloud sync clients when the platform supports it.
pub fn mark_path_ignored_by_cloud_sync(path: &str) {
    #[cfg(target_os = "macos")]
    {
        for attribute in ["com.dropbox.ignored", "com.apple.fileprovider.ignore#P"] {
            let _ = std::process::Command::new("xattr")
                .args(["-w", attribute, "1", path])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("setfattr")
            .args(["-n", "user.com.dropbox.ignored", "-v", "1", path])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
    }
}

fn resolve_from_process_cwd(path: &str) -> Result<String, PathError> {
    let normalized = normalize_path(path, &PathInputOptions::default())?;
    if Path::new(&normalized).is_absolute() {
        return Ok(lexical_normalize(&PathBuf::from(normalized)));
    }
    Ok(node_resolve(&current_directory(), &normalized))
}

fn current_directory() -> String {
    std::env::current_dir()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_owned())
}

/// `process.cwd()` as a display string.
pub(crate) fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn node_resolve(base: &str, tail: &str) -> String {
    let mut resolved = PathBuf::from(base);
    resolved.push(tail);
    lexical_normalize(&resolved)
}

pub(crate) fn node_join(base: &str, tail: &str) -> String {
    lexical_normalize(Path::new(&format!("{base}/{tail}")))
}

pub(crate) fn lexical_normalize(path: &Path) -> String {
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
    normalized.to_string_lossy().into_owned()
}

pub fn relative_path(from: &str, to: &str) -> String {
    let from_components = Path::new(from).components().collect::<Vec<_>>();
    let to_components = Path::new(to).components().collect::<Vec<_>>();

    if !same_root(&from_components, &to_components) {
        return to.to_owned();
    }

    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();

    let mut relative = PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    relative.to_string_lossy().into_owned()
}

fn same_root(left: &[Component<'_>], right: &[Component<'_>]) -> bool {
    let left_root = left
        .first()
        .is_some_and(|component| matches!(component, Component::RootDir));
    let right_root = right
        .first()
        .is_some_and(|component| matches!(component, Component::RootDir));
    if left_root != right_root {
        return false;
    }
    let left_prefix = left.first().and_then(|component| match component {
        Component::Prefix(prefix) => Some(prefix.kind()),
        _ => None,
    });
    let right_prefix = right.first().and_then(|component| match component {
        Component::Prefix(prefix) => Some(prefix.kind()),
        _ => None,
    });
    left_prefix == right_prefix
}

fn home_directory() -> String {
    #[cfg(windows)]
    {
        pi_js_compat::process_env::get("USERPROFILE").unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        pi_js_compat::process_env::get("HOME").unwrap_or_default()
    }
}

fn is_unicode_space(character: char) -> bool {
    matches!(
        character,
        '\u{a0}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

fn file_url_to_path(value: &str) -> Result<String, PathError> {
    let url = url::Url::parse(value).map_err(|error| PathError::new(error.to_string()))?;
    if url.scheme() != "file" {
        return Err(PathError::new("not a file URL"));
    }

    if !cfg!(windows) {
        match url.host_str() {
            None | Some("") | Some("localhost") => {}
            Some(host) => {
                return Err(PathError::new(format!(
                    "file URL host must be 'localhost' or empty, got {host}"
                )));
            }
        }
    }

    let encoded_path = url.path();
    if encoded_path.contains("%2F") || encoded_path.contains("%2f") {
        return Err(PathError::new("file URL must not include encoded '/' characters"));
    }
    if !has_valid_percent_sequences(encoded_path) {
        return Err(PathError::new("URI malformed"));
    }
    let decoded = percent_decode_str(encoded_path)
        .decode_utf8()
        .map_err(|error| PathError::new(error.to_string()))?
        .into_owned();

    #[cfg(windows)]
    {
        let bytes = decoded.as_bytes();
        if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
            return Ok(decoded[1..].to_owned());
        }
    }

    Ok(decoded)
}

fn has_valid_percent_sequences(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
            continue;
        }
        index += 1;
    }
    true
}
