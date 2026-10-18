//! Port of `packages/coding-agent/src/utils/changelog.ts`.

use std::path::Path;

use regex::Regex;

pub use crate::config::get_changelog_path;

const GITHUB_REPO: &str = "earendil-works/pi";
const CHANGELOG_LINK_BASE_PATH: &str = "packages/coding-agent";

/// One `## x.y.z` section of `CHANGELOG.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogEntry {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub content: String,
}

fn normalize_tag(version: &str) -> String {
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

struct LocalTarget {
    fragment: String,
    path_part: String,
    query: String,
}

fn split_local_target(target: &str) -> LocalTarget {
    let (before_hash, fragment) = match target.find('#') {
        Some(index) => (&target[..index], &target[index..]),
        None => (target, ""),
    };
    match before_hash.find('?') {
        Some(index) => LocalTarget {
            fragment: fragment.to_owned(),
            path_part: before_hash[..index].to_owned(),
            query: before_hash[index..].to_owned(),
        },
        None => LocalTarget {
            fragment: fragment.to_owned(),
            path_part: before_hash.to_owned(),
            query: String::new(),
        },
    }
}

fn normalize_path_part(value: &str) -> String {
    value.replace('\\', "/")
}

/// `path.posix.normalize` for the relative repository paths used by changelog links.
fn posix_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let trailing_slash = path.len() > 1 && path.ends_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    let normalized = if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    };
    if trailing_slash && normalized != "/" && !normalized.ends_with('/') {
        return format!("{normalized}/");
    }
    normalized
}

fn resolve_repository_path(target_path: &str) -> Option<String> {
    let normalized_target = normalize_path_part(target_path);
    let joined = if let Some(stripped) = normalized_target.strip_prefix('/') {
        posix_normalize(stripped.trim_start_matches('/'))
    } else {
        posix_normalize(&format!("{CHANGELOG_LINK_BASE_PATH}/{normalized_target}"))
    };
    if joined == "." || joined == ".." || joined.starts_with("../") {
        return None;
    }
    Some(joined)
}

fn is_directory_target(original_path: &str, repository_path: &str) -> bool {
    if original_path.ends_with('/') {
        return true;
    }
    let basename = repository_path.rsplit('/').next().unwrap_or(repository_path);
    !basename.contains('.')
}

/// `encodeURI`: escapes everything except URI syntax and unreserved characters.
fn encode_uri(value: &str) -> String {
    const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'();/?:@&=+$,#";
    let mut encoded = String::new();
    for byte in value.bytes() {
        if UNRESERVED.contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Replaces `needle` when it ends at a path separator or the end of the string
/// (the Rust regex crate has no look-ahead).
fn replace_path_segment(haystack: &str, needle: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut rest = haystack;
    while let Some(index) = rest.find(needle) {
        let after = index + needle.len();
        let boundary = rest[after..].is_empty() || rest[after..].starts_with('/');
        result.push_str(&rest[..index]);
        if boundary {
            result.push_str(replacement);
            rest = &rest[after..];
        } else {
            result.push_str(needle);
            rest = &rest[after..];
        }
    }
    result.push_str(rest);
    result
}

fn normalize_changelog_link_target(target: &str, tag: &str) -> String {
    let repo_url = format!("https://github.com/{GITHUB_REPO}");
    let mut canonical_target = target.to_owned();
    for legacy_prefix in [
        "https://github.com/badlogic/pi-mono",
        "https://github.com/earendil-works/pi-mono",
    ] {
        if let Some(remainder) = canonical_target.strip_prefix(legacy_prefix)
            && (remainder.is_empty() || remainder.starts_with('/'))
        {
            canonical_target = format!("{repo_url}{remainder}");
            break;
        }
    }

    if canonical_target.starts_with(&format!("{repo_url}/blob/"))
        || canonical_target.starts_with(&format!("{repo_url}/tree/"))
    {
        canonical_target = replace_path_segment(&canonical_target, "/packages/agent", "/packages/agent-core");
    }

    for route in ["blob", "tree"] {
        for branch in ["main", "master"] {
            let floating_ref_prefix = format!("{repo_url}/{route}/{branch}/");
            if let Some(remainder) = canonical_target.strip_prefix(&floating_ref_prefix) {
                canonical_target = format!("{repo_url}/{route}/{tag}/{remainder}");
            }
        }
    }

    let scheme_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*:").expect("static regex");
    if canonical_target.starts_with('#')
        || canonical_target.starts_with("//")
        || scheme_regex.is_match(&canonical_target)
    {
        return canonical_target;
    }

    let LocalTarget {
        fragment,
        path_part,
        query,
    } = split_local_target(&canonical_target);
    if path_part.is_empty() {
        return canonical_target;
    }
    let Some(repository_path) = resolve_repository_path(&path_part) else {
        return canonical_target;
    };
    let route = if is_directory_target(&path_part, &repository_path) {
        "tree"
    } else {
        "blob"
    };
    format!(
        "https://github.com/{GITHUB_REPO}/{route}/{tag}/{}",
        encode_uri(&repository_path)
    ) + &query
        + &fragment
}

/// Rewrites inline changelog links to tag-pinned GitHub URLs.
pub fn normalize_changelog_links(markdown: &str, version: &str) -> String {
    normalize_changelog_links_for_entry(markdown, version)
}

/// Same as [`normalize_changelog_links`] for a parsed entry.
pub fn normalize_changelog_links_for_entry(markdown: &str, version: &str) -> String {
    let tag = normalize_tag(version);
    let link_regex = Regex::new(r"(!?\[[^\]\n]+\]\()([^\s)]+)((?:\s+[^)]*)?\))").expect("static regex");
    link_regex
        .replace_all(markdown, |captures: &regex::Captures<'_>| {
            format!(
                "{}{}{}",
                &captures[1],
                normalize_changelog_link_target(&captures[2], &tag),
                &captures[3]
            )
        })
        .into_owned()
}

/// Parses `## x.y.z` entries from `CHANGELOG.md`.
pub fn parse_changelog(changelog_path: &str) -> Vec<ChangelogEntry> {
    if !Path::new(changelog_path).exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(changelog_path) {
        Ok(content) => content,
        Err(error) => {
            eprintln!("Warning: Could not parse changelog: {error}");
            return Vec::new();
        }
    };
    let version_regex = Regex::new(r"##\s+\[?(\d+)\.(\d+)\.(\d+)\]?").expect("static regex");
    let mut entries = Vec::new();
    let mut current_lines: Vec<String> = Vec::new();
    let mut current_version: Option<(u64, u64, u64)> = None;

    for line in content.split('\n') {
        if line.starts_with("## ") {
            if let Some((major, minor, patch)) = current_version
                && !current_lines.is_empty()
            {
                entries.push(ChangelogEntry {
                    major,
                    minor,
                    patch,
                    content: current_lines.join("\n").trim().to_owned(),
                });
            }
            match version_regex.captures(line) {
                Some(captures) => {
                    current_version = Some((
                        captures[1].parse().unwrap_or(0),
                        captures[2].parse().unwrap_or(0),
                        captures[3].parse().unwrap_or(0),
                    ));
                    current_lines = vec![line.to_owned()];
                }
                None => {
                    current_version = None;
                    current_lines.clear();
                }
            }
        } else if current_version.is_some() {
            current_lines.push(line.to_owned());
        }
    }

    if let Some((major, minor, patch)) = current_version
        && !current_lines.is_empty()
    {
        entries.push(ChangelogEntry {
            major,
            minor,
            patch,
            content: current_lines.join("\n").trim().to_owned(),
        });
    }
    entries
}

/// Orders entries by version.
pub fn compare_versions(left: &ChangelogEntry, right: &ChangelogEntry) -> i64 {
    if left.major != right.major {
        return left.major as i64 - right.major as i64;
    }
    if left.minor != right.minor {
        return left.minor as i64 - right.minor as i64;
    }
    left.patch as i64 - right.patch as i64
}

/// Entries newer than `last_version` (`x.y.z`, missing parts are zero).
pub fn get_new_entries(entries: &[ChangelogEntry], last_version: &str) -> Vec<ChangelogEntry> {
    let parts: Vec<u64> = last_version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect();
    let last = ChangelogEntry {
        major: parts.first().copied().unwrap_or(0),
        minor: parts.get(1).copied().unwrap_or(0),
        patch: parts.get(2).copied().unwrap_or(0),
        content: String::new(),
    };
    entries
        .iter()
        .filter(|entry| compare_versions(entry, &last) > 0)
        .cloned()
        .collect()
}
