use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;

/// Options for the source `utils/paths.ts` input normalization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathInputOptions {
    pub trim: bool,
    pub expand_tilde: Option<bool>,
    pub home_dir: Option<JsString>,
    pub strip_at_prefix: bool,
    pub normalize_unicode_spaces: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathError {
    message: JsString,
    native: Option<Box<NativeNodeError>>,
}

impl PathError {
    fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            native: None,
        }
    }

    pub fn native_error(&self) -> Option<&NativeNodeError> {
        self.native.as_deref()
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}
impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.native_error().map(|error| error as &dyn std::error::Error)
    }
}
impl From<NativeNodeError> for PathError {
    fn from(error: NativeNodeError) -> Self {
        Self {
            message: error.message.clone(),
            native: Some(Box::new(error)),
        }
    }
}

/// The source explicitly returns the original string after a realpath failure.
pub fn canonicalize_path(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    let resolved = match node_resolve("", &path) {
        Ok(resolved) => resolved,
        Err(_) => return path,
    };
    read_realpath(resolved).unwrap_or(path)
}

/// Node's JS realpath walk retains input units in non-symlink components.
fn read_realpath(mut path: JsString) -> Result<JsString, PathError> {
    let mut known_hard = std::collections::HashSet::new();
    let mut seen_links: std::collections::HashMap<(u64, u64), JsString> = std::collections::HashMap::new();
    let mut current = JsString::from("/");
    let mut position = 1;
    let mut reached_pipe_or_socket = false;
    while position < path.len() {
        let previous = current;
        let next = path.find_from('/', position);
        let end = next.unwrap_or(path.len());
        let base = path.slice(0, end);
        position = next.map_or(path.len(), |index| index + 1);
        current = path.slice(0, position);
        if known_hard.contains(&base) {
            if reached_pipe_or_socket {
                break;
            }
            continue;
        }
        let encoded = encode_node_utf8(&base);
        let metadata = std::fs::symlink_metadata(&encoded).map_err(|error| PathError::new(error.to_string()))?;
        if !metadata.file_type().is_symlink() {
            known_hard.insert(base);
            continue;
        }
        let identity = link_identity(&metadata);
        let target = if let Some(target) = identity.and_then(|identity| seen_links.get(&identity)) {
            target.clone()
        } else {
            let target_metadata = std::fs::metadata(&encoded).map_err(|error| PathError::new(error.to_string()))?;
            reached_pipe_or_socket = is_pipe_or_socket(&target_metadata);
            let target = std::fs::read_link(&encoded).map_err(|error| PathError::new(error.to_string()))?;
            let target = JsString::from(target.to_string_lossy().as_ref());
            if let Some(identity) = identity {
                seen_links.insert(identity, target.clone());
            }
            target
        };
        let resolved = node_resolve(previous, target)?;
        path = node_resolve(resolved, path.slice(position, path.len()))?;
        current = "/".into();
        position = 1;
    }
    Ok(path)
}

fn link_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn is_pipe_or_socket(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        metadata.file_type().is_fifo() || metadata.file_type().is_socket()
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

pub fn is_local_path(value: impl Into<JsString>) -> bool {
    let trimmed = value.into().trim();
    !["npm:", "git:", "github:", "http:", "https:", "ssh:"]
        .iter()
        .any(|prefix| trimmed.starts_with(*prefix))
}

pub fn normalize_path(input: impl Into<JsString>, options: &PathInputOptions) -> Result<JsString, PathError> {
    let input = input.into();
    let mut normalized = if options.trim { input.trim() } else { input };
    if options.normalize_unicode_spaces {
        normalized = JsString::from_utf16(
            normalized
                .units()
                .iter()
                .map(|unit| {
                    if matches!(unit, 0x00a0 | 0x2000..=0x200a | 0x202f | 0x205f | 0x3000) {
                        0x20
                    } else {
                        *unit
                    }
                })
                .collect(),
        );
    }
    if options.strip_at_prefix && normalized.starts_with('@') {
        normalized = normalized.slice(1, normalized.len());
    }
    if options.expand_tilde.unwrap_or(true) {
        let home = options.home_dir.clone().map(Ok).unwrap_or_else(home_directory)?;
        if normalized == "~" {
            return Ok(home);
        }
        let prefix = if cfg!(windows) && normalized.starts_with("~\\") {
            "~\\"
        } else {
            "~/"
        };
        if let Some(remainder) = normalized.strip_prefix(prefix) {
            return Ok(node_join(home, remainder));
        }
    }
    if normalized.starts_with("file://") {
        return file_url_to_path(&normalized);
    }
    Ok(normalized)
}

pub fn resolve_path(
    input: impl Into<JsString>,
    base_dir: impl Into<JsString>,
    options: &PathInputOptions,
) -> Result<JsString, PathError> {
    let normalized = normalize_path(input, options)?;
    let base = normalize_path(base_dir, &PathInputOptions::default())?;
    node_resolve(base, normalized)
}

pub fn get_cwd_relative_path(
    file_path: impl Into<JsString>,
    cwd: impl Into<JsString>,
) -> Result<Option<JsString>, PathError> {
    let resolved_cwd = resolve_path(cwd, process_cwd()?, &PathInputOptions::default())?;
    let resolved_path = resolve_path(file_path, &resolved_cwd, &PathInputOptions::default())?;
    let relative = relative_path(&resolved_cwd, &resolved_path)?;
    let inside =
        relative.is_empty() || (relative != ".." && !relative.starts_with("../") && !node_is_absolute(&relative));
    Ok(inside.then(|| if relative.is_empty() { ".".into() } else { relative }))
}

pub fn format_path_relative_to_cwd_or_absolute(
    file_path: impl Into<JsString>,
    cwd: impl Into<JsString>,
) -> Result<JsString, PathError> {
    let cwd = cwd.into();
    let absolute = resolve_path(file_path, &cwd, &PathInputOptions::default())?;
    Ok(get_cwd_relative_path(&absolute, cwd)?.unwrap_or(absolute))
}

/// The actual argv binding, rather than the normalization functions, encodes text.
pub fn mark_path_ignored_by_cloud_sync(path: impl Into<JsString>) -> Result<(), Box<NativeNodeError>> {
    use super::child_process::{ChildStdio, ChildStdioOptions, SpawnSyncOptions, spawn_sync};
    let options = || SpawnSyncOptions {
        stdio: ChildStdioOptions {
            stdin: ChildStdio::Ignore,
            stdout: ChildStdio::Ignore,
            stderr: ChildStdio::Ignore,
        },
        ..Default::default()
    };
    let path = path.into();
    #[cfg(target_os = "macos")]
    for attribute in ["com.dropbox.ignored", "com.apple.fileprovider.ignore#P"] {
        spawn_sync(
            &"xattr".into(),
            &["-w".into(), attribute.into(), "1".into(), path.clone()],
            options(),
        )?;
    }
    #[cfg(target_os = "linux")]
    {
        spawn_sync(
            &"setfattr".into(),
            &[
                "-n".into(),
                "user.com.dropbox.ignored".into(),
                "-v".into(),
                "1".into(),
                path,
            ],
            options(),
        )?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (path, options);
    }
    Ok(())
}

pub fn process_cwd() -> Result<JsString, PathError> {
    pi_js_compat::process_cwd::cwd().map_err(PathError::from)
}

/// Native POSIX path operations. Separators/dot components are ASCII; all other units are opaque.
pub fn node_is_absolute(path: impl Into<JsString>) -> bool {
    path.into().starts_with('/')
}

pub fn node_normalize(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    if path.is_empty() {
        return ".".into();
    }
    let absolute = node_is_absolute(&path);
    let trailing = path.ends_with('/');
    let mut parts: Vec<JsString> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.last().is_some_and(|last| last != "..") {
                parts.pop();
            } else if !absolute {
                parts.push(part);
            }
        } else {
            parts.push(part);
        }
    }
    let mut result = JsString::join(parts, "/");
    if absolute {
        result = JsString::from("/") + result;
    }
    if result.is_empty() {
        result = ".".into();
    }
    if trailing && result != "/" {
        result.push_str("/");
    }
    result
}

pub fn node_join(base: impl Into<JsString>, tail: impl Into<JsString>) -> JsString {
    let base = base.into();
    let tail = tail.into();
    if base.is_empty() {
        return node_normalize(tail);
    }
    if tail.is_empty() {
        return node_normalize(base);
    }
    node_normalize(base + "/" + tail)
}

pub fn node_resolve(base: impl Into<JsString>, tail: impl Into<JsString>) -> Result<JsString, PathError> {
    let base = base.into();
    let tail = tail.into();
    let joined = if node_is_absolute(&tail) || base.is_empty() {
        tail
    } else {
        base + "/" + tail
    };
    let joined = if node_is_absolute(&joined) {
        joined
    } else {
        process_cwd()? + "/" + joined
    };
    let normalized = node_normalize(joined);
    Ok(if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.slice(0, normalized.len() - 1)
    } else {
        normalized
    })
}

pub fn relative_path(from: impl Into<JsString>, to: impl Into<JsString>) -> Result<JsString, PathError> {
    let from = node_resolve("", from)?;
    let to = node_resolve("", to)?;
    let from: Vec<_> = from.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<_> = to.split('/').filter(|part| !part.is_empty()).collect();
    let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
    Ok(JsString::join(
        std::iter::repeat_n(JsString::from(".."), from.len() - common).chain(to.into_iter().skip(common)),
        "/",
    ))
}

pub fn node_dirname(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    let end = path
        .units()
        .iter()
        .rposition(|unit| *unit != u16::from(b'/'))
        .map_or(0, |index| index + 1);
    let Some(slash) = path.units()[..end].iter().rposition(|unit| *unit == u16::from(b'/')) else {
        return if node_is_absolute(&path) {
            "/".into()
        } else {
            ".".into()
        };
    };
    if slash == 0 {
        return "/".into();
    }
    if slash == 1 && path.starts_with("//") {
        return "//".into();
    }
    path.slice(0, slash)
}

pub fn node_basename(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    let end = path
        .units()
        .iter()
        .rposition(|unit| *unit != u16::from(b'/'))
        .map_or(0, |index| index + 1);
    let start = path.units()[..end]
        .iter()
        .rposition(|unit| *unit == u16::from(b'/'))
        .map_or(0, |index| index + 1);
    path.slice(start, end)
}

/// POSIX `path.extname`: the last `.` suffix of the basename, unless that dot leads it.
pub fn node_extname(path: impl Into<JsString>) -> JsString {
    let name = node_basename(path);
    if name == ".." {
        return JsString::default();
    }
    match name.units().iter().rposition(|unit| *unit == u16::from(b'.')) {
        Some(dot) if dot > 0 => name.slice(dot, name.len()),
        _ => JsString::default(),
    }
}

pub fn home_directory() -> Result<JsString, NativeNodeError> {
    pi_js_compat::node_os::homedir()
}

fn file_url_to_path(value: &JsString) -> Result<JsString, PathError> {
    let file = pi_js_compat::node_url::FileUrl::parse(value);
    // Existing IDNA/host validation only. This parser never owns the pathname.
    let host_url = format!("file://{}/", file.host());
    let url = url::Url::parse(&host_url).map_err(|_| {
        PathError::from(
            NativeNodeError::from_constructor(pi_js_compat::node_error::NativeErrorClass::TypeError, "Invalid URL")
                .with_property("code", pi_js_compat::js_json::JsValue::String("ERR_INVALID_URL".into()))
                .with_property(
                    "input",
                    pi_js_compat::js_json::JsValue::String(encode_node_utf8(value).into()),
                ),
        )
    })?;
    file.into_posix_path(url.host_str().unwrap_or(""))
        .map_err(PathError::from)
}
