use percent_encoding::percent_decode_str;
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use serde::{Deserialize, Serialize};
use url::Url;

/// Parsed git URL information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSource {
    /// Always `git` for git sources.
    #[serde(rename = "type")]
    pub source_type: GitSourceType,
    /// Clone URL without a ref suffix.
    pub repo: JsString,
    /// Git host domain, for example `github.com`.
    pub host: JsString,
    /// Repository path, for example `user/repo`.
    pub path: JsString,
    /// Git ref (branch, tag, commit) when specified.
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<JsString>,
    /// True when a ref was specified.
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitSourceType {
    Git,
}

/// Parse a git source URL.
///
/// With a `git:` prefix all historical shorthand forms are accepted. Without
/// the prefix only explicit protocol URLs are accepted.
pub fn parse_git_url(source: &JsString) -> Option<GitSource> {
    let trimmed = source.trim();
    let has_git_prefix = trimmed.starts_with("git:");
    let url = if has_git_prefix {
        trimmed.slice(4, trimmed.len()).trim()
    } else {
        trimmed.clone()
    };

    if !has_git_prefix && !has_explicit_protocol(&url) {
        return None;
    }

    let split = split_ref(&url);
    if let Some(source) = parse_hosted_candidates(&split, &url) {
        return Some(source);
    }
    if let Some(source) = parse_https_candidates(&split, &url) {
        return Some(source);
    }
    parse_generic_git_url(&url)
}

fn parse_hosted_candidates(split: &SplitRef, url: &JsString) -> Option<GitSource> {
    for candidate in reference_candidates(split, url) {
        let Some(info) = hosted_git_info(&candidate) else {
            continue;
        };
        if split.reference.is_some() && info.project.contains('@') {
            continue;
        }
        let repo = if needs_https_prefix(&split.repo) {
            js_format!("https://{}", split.repo)
        } else {
            split.repo.clone()
        };
        return build_from_hosted_info(repo, info, split);
    }
    None
}

fn parse_https_candidates(split: &SplitRef, url: &JsString) -> Option<GitSource> {
    for candidate in https_candidates(split, url) {
        let Some(info) = hosted_git_info(&candidate) else {
            continue;
        };
        if split.reference.is_some() && info.project.contains('@') {
            continue;
        }
        return build_from_hosted_info(js_format!("https://{}", split.repo), info, split);
    }
    None
}

fn reference_candidates(split: &SplitRef, url: &JsString) -> Vec<JsString> {
    let mut candidates = Vec::new();
    if let Some(reference) = &split.reference {
        candidates.push(js_format!("{}#{}", split.repo, reference));
    }
    candidates.push(url.to_owned());
    candidates
}

fn https_candidates(split: &SplitRef, url: &JsString) -> Vec<JsString> {
    let mut candidates = Vec::new();
    if let Some(reference) = &split.reference {
        candidates.push(js_format!("https://{}#{}", split.repo, reference));
    }
    candidates.push(js_format!("https://{}", url));
    candidates
}

fn build_from_hosted_info(repo: JsString, info: HostedGitInfo, split: &SplitRef) -> Option<GitSource> {
    let user = info.user.unwrap_or_else(|| "null".to_owned());
    let path = js_format!("{}/{}", user, info.project);
    let reference = info.committish.map(JsString::from).or_else(|| split.reference.clone());
    build_git_source(repo, info.domain.into(), path, reference)
}

fn has_explicit_protocol(url: &JsString) -> bool {
    let lowercase = url.to_lowercase();
    ["http://", "https://", "ssh://", "git://"]
        .iter()
        .any(|prefix| lowercase.starts_with(*prefix))
}

fn needs_https_prefix(repo: &JsString) -> bool {
    !(repo.starts_with("http://")
        || repo.starts_with("https://")
        || repo.starts_with("ssh://")
        || repo.starts_with("git://")
        || repo.starts_with("git@"))
}

struct SplitRef {
    repo: JsString,
    reference: Option<JsString>,
}

fn split_ref(url: &JsString) -> SplitRef {
    if let Some(split) = split_scp_like_ref(url) {
        return split;
    }
    if url.contains("://") {
        return split_url_ref(url);
    }
    split_host_path_ref(url)
}

fn split_scp_like_ref(url: &JsString) -> Option<SplitRef> {
    let rest = url.strip_prefix("git@")?;
    let (host, path_with_maybe_ref) = split_once(&rest, ':')?;
    if path_with_maybe_ref.is_empty() {
        return None;
    }
    let (repo_path, reference) = split_reference(&path_with_maybe_ref)?;
    Some(SplitRef {
        repo: js_format!("git@{}:{}", host, repo_path),
        reference: Some(reference.to_owned()),
    })
}

fn split_url_ref(url: &JsString) -> SplitRef {
    let Ok(mut parsed) = Url::parse(&encode_node_utf8(url)) else {
        return unchanged_ref(url);
    };
    let path_with_maybe_ref = JsString::from(parsed.path().trim_start_matches('/'));
    let Some((repo_path, reference)) = split_reference(&path_with_maybe_ref) else {
        return unchanged_ref(url);
    };

    parsed.set_path(&encode_node_utf8(&js_format!("/{}", repo_path)));
    SplitRef {
        repo: parsed.to_string().trim_end_matches('/').into(),
        reference: Some(reference),
    }
}

fn split_host_path_ref(url: &JsString) -> SplitRef {
    let Some(slash_index) = url.find('/') else {
        return unchanged_ref(url);
    };
    let host = url.slice(0, slash_index);
    let path_with_maybe_ref = url.slice(slash_index + 1, url.len());
    let Some((repo_path, reference)) = split_reference(&path_with_maybe_ref) else {
        return unchanged_ref(url);
    };
    SplitRef {
        repo: js_format!("{}/{}", host, repo_path),
        reference: Some(reference.to_owned()),
    }
}

fn split_reference(path_with_maybe_ref: &JsString) -> Option<(JsString, JsString)> {
    let separator = path_with_maybe_ref.find('@')?;
    let repo_path = path_with_maybe_ref.slice(0, separator);
    let reference = path_with_maybe_ref.slice(separator + 1, path_with_maybe_ref.len());
    if repo_path.is_empty() || reference.is_empty() {
        return None;
    }
    Some((repo_path, reference))
}

fn unchanged_ref(url: &JsString) -> SplitRef {
    SplitRef {
        repo: url.to_owned(),
        reference: None,
    }
}

struct HostedGitInfo {
    domain: String,
    user: Option<String>,
    project: String,
    committish: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Host {
    Github,
    Bitbucket,
    Gitlab,
    Gist,
    Sourcehut,
}

impl Host {
    fn from_domain(domain: &str) -> Option<Self> {
        match domain {
            "github.com" => Some(Self::Github),
            "bitbucket.org" => Some(Self::Bitbucket),
            "gitlab.com" => Some(Self::Gitlab),
            "gist.github.com" => Some(Self::Gist),
            "git.sr.ht" => Some(Self::Sourcehut),
            _ => None,
        }
    }

    fn from_shortcut(protocol: &str) -> Option<Self> {
        match protocol {
            "github" => Some(Self::Github),
            "bitbucket" => Some(Self::Bitbucket),
            "gitlab" => Some(Self::Gitlab),
            "gist" => Some(Self::Gist),
            "sourcehut" => Some(Self::Sourcehut),
            _ => None,
        }
    }

    fn domain(self) -> &'static str {
        match self {
            Self::Github => "github.com",
            Self::Bitbucket => "bitbucket.org",
            Self::Gitlab => "gitlab.com",
            Self::Gist => "gist.github.com",
            Self::Sourcehut => "git.sr.ht",
        }
    }

    fn accepts_protocol(self, protocol: &str) -> bool {
        match self {
            Self::Github => matches!(
                protocol,
                "git:" | "http:" | "git+ssh:" | "git+https:" | "ssh:" | "https:"
            ),
            Self::Bitbucket | Self::Gitlab => {
                matches!(protocol, "git+ssh:" | "git+https:" | "ssh:" | "https:")
            }
            Self::Gist => matches!(protocol, "git:" | "git+ssh:" | "git+https:" | "ssh:" | "https:"),
            Self::Sourcehut => matches!(protocol, "git+ssh:" | "https:"),
        }
    }
}

fn hosted_git_info(giturl: &JsString) -> Option<HostedGitInfo> {
    // hosted-git-info parses candidates as URLs; URL input applies USVString conversion.
    let encoded = encode_node_utf8(giturl);
    let giturl = encoded.as_str();
    let corrected = if is_github_shorthand(giturl) {
        format!("github:{giturl}")
    } else {
        giturl.to_owned()
    };
    let parsed = parse_url(&corrected)?;

    let shortcut = Host::from_shortcut(parsed.scheme());
    let parsed_host = parsed.host_str().unwrap_or("");
    let domain = parsed_host.strip_prefix("www.").unwrap_or(parsed_host);
    let host = shortcut.or_else(|| Host::from_domain(domain))?;

    let (user, project, committish) = if shortcut.is_some() {
        extract_shortcut(&parsed)?
    } else {
        if !host.accepts_protocol(&protocol_with_colon(parsed.scheme())) {
            return None;
        }
        extract_host(host, &parsed)?
    };

    Some(HostedGitInfo {
        domain: host.domain().to_owned(),
        user,
        project,
        committish: committish.filter(|value| !value.is_empty()),
    })
}

fn protocol_with_colon(scheme: &str) -> String {
    format!("{scheme}:")
}

fn extract_shortcut(parsed: &Url) -> Option<(Option<String>, String, Option<String>)> {
    let path = parsed.path();
    let mut pathname = path.strip_prefix('/').unwrap_or(path).to_owned();
    if let Some(first_at) = pathname.find('@') {
        pathname = pathname[first_at + 1..].to_owned();
    }

    let (user, project) = match pathname.rfind('/') {
        Some(last_slash) => {
            let user = decode_uri_component(&pathname[..last_slash]).filter(|value| !value.is_empty());
            let project = decode_uri_component(&pathname[last_slash + 1..])?;
            (user, project)
        }
        None => (None, decode_uri_component(&pathname)?),
    };

    let project = project.strip_suffix(".git").unwrap_or(&project).to_owned();
    let committish = parsed.fragment().and_then(decode_uri_component);
    Some((user, project, committish))
}

type ExtractedSegments = (Option<String>, String, Option<String>);

fn extract_host(host: Host, parsed: &Url) -> Option<ExtractedSegments> {
    let pathname = parsed.path();
    let hash = parsed.fragment().unwrap_or("");
    match host {
        Host::Github => extract_github(pathname, hash),
        Host::Bitbucket => extract_bitbucket(pathname, hash),
        Host::Gitlab => extract_gitlab(pathname, hash),
        Host::Gist => extract_gist(pathname, hash),
        Host::Sourcehut => extract_sourcehut(pathname, hash),
    }
}

fn extract_github(pathname: &str, hash: &str) -> Option<ExtractedSegments> {
    let segments = split_limited(pathname, 5);
    let user = segments.get(1).copied().unwrap_or("");
    let project = segments.get(2).copied().unwrap_or("");
    let kind = segments.get(3).copied().filter(|value| !value.is_empty());
    if kind.is_some_and(|kind| kind != "tree") {
        return None;
    }
    let committish = if kind.is_none() {
        hash
    } else {
        segments.get(4).copied().unwrap_or("")
    };
    if user.is_empty() || project.is_empty() {
        return None;
    }
    let project = project.strip_suffix(".git").unwrap_or(project);
    decoded_segments(Some(user), project, committish)
}

fn extract_bitbucket(pathname: &str, hash: &str) -> Option<ExtractedSegments> {
    extract_user_project(pathname, hash, "get")
}

fn extract_sourcehut(pathname: &str, hash: &str) -> Option<ExtractedSegments> {
    extract_user_project(pathname, hash, "archive")
}

fn extract_user_project(pathname: &str, hash: &str, rejected_aux: &str) -> Option<ExtractedSegments> {
    let segments = split_limited(pathname, 4);
    let user = segments.get(1).copied().unwrap_or("");
    let project = segments.get(2).copied().unwrap_or("");
    if segments.get(3).copied() == Some(rejected_aux) {
        return None;
    }
    if user.is_empty() || project.is_empty() {
        return None;
    }
    let project = project.strip_suffix(".git").unwrap_or(project);
    decoded_segments(Some(user), project, hash)
}

fn extract_gitlab(pathname: &str, hash: &str) -> Option<ExtractedSegments> {
    let path = pathname.strip_prefix('/').unwrap_or(pathname);
    if path.contains("/-/") || path.contains("/archive.tar.gz") {
        return None;
    }
    let mut segments = path.split('/').collect::<Vec<_>>();
    let project = segments.pop()?;
    let user = segments.join("/");
    if user.is_empty() || project.is_empty() {
        return None;
    }
    let project = project.strip_suffix(".git").unwrap_or(project);
    decoded_segments(Some(&user), project, hash)
}

fn extract_gist(pathname: &str, hash: &str) -> Option<ExtractedSegments> {
    let segments = split_limited(pathname, 4);
    let mut user = segments.get(1).copied();
    let mut project = segments.get(2).copied();
    if segments.get(3).copied() == Some("raw") {
        return None;
    }
    if project.is_none() {
        project = Some(user?);
        user = None;
    }
    let project = project?;
    let project = project.strip_suffix(".git").unwrap_or(project);
    decoded_segments(user, project, hash)
}

fn decoded_segments(user: Option<&str>, project: &str, committish: &str) -> Option<ExtractedSegments> {
    let user = match user {
        Some(value) => Some(decode_uri_component(value)?),
        None => None,
    };
    Some((
        user,
        decode_uri_component(project)?,
        Some(decode_uri_component(committish)?),
    ))
}

fn split_limited(pathname: &str, limit: usize) -> Vec<&str> {
    pathname.split('/').take(limit).collect()
}

fn is_github_shorthand(argument: &str) -> bool {
    let first_hash = argument.find('#');
    let first_slash = argument.find('/');
    let second_slash = first_slash.and_then(|index| argument[index + 1..].find('/').map(|offset| index + 1 + offset));
    let first_colon = argument.find(':');
    let first_space = argument.find(char::is_whitespace);
    let first_at = argument.find('@');

    let space_only_after_hash = first_space.is_none() || first_hash.is_some_and(|hash| first_space.unwrap() > hash);
    let at_only_after_hash = first_at.is_none() || first_hash.is_some_and(|hash| first_at.unwrap() > hash);
    let colon_only_after_hash = first_colon.is_none() || first_hash.is_some_and(|hash| first_colon.unwrap() > hash);
    let second_slash_only_after_hash =
        second_slash.is_none() || first_hash.is_some_and(|hash| second_slash.unwrap() > hash);
    let has_slash = first_slash.is_some_and(|index| index > 0);
    let does_not_end_with_slash = match first_hash {
        Some(hash) => argument.as_bytes().get(hash.wrapping_sub(1)) != Some(&b'/'),
        None => !argument.ends_with('/'),
    };
    let does_not_start_with_dot = !argument.starts_with('.');

    space_only_after_hash
        && has_slash
        && does_not_end_with_slash
        && does_not_start_with_dot
        && at_only_after_hash
        && colon_only_after_hash
        && second_slash_only_after_hash
}

fn parse_url(giturl: &str) -> Option<Url> {
    let with_protocol = correct_protocol(giturl);
    Url::parse(&with_protocol)
        .ok()
        .or_else(|| Url::parse(&correct_url(&with_protocol)).ok())
}

fn correct_protocol(argument: &str) -> String {
    let first_colon = argument.find(':');
    if let Some(first_colon) = first_colon {
        let protocol = &argument[..=first_colon];
        if is_known_protocol(protocol) {
            return argument.to_owned();
        }
        if argument.get(first_colon..first_colon + 3) == Some("://") {
            return argument.to_owned();
        }
    }

    let first_at = argument.find('@');
    if let Some(first_at) = first_at {
        if first_colon.is_some_and(|colon| first_at > colon) {
            return format!("git+ssh://{argument}");
        }
        return argument.to_owned();
    }

    match first_colon {
        Some(first_colon) => format!("{}//{}", &argument[..=first_colon], &argument[first_colon + 1..]),
        None => format!("//{argument}"),
    }
}

fn is_known_protocol(protocol: &str) -> bool {
    matches!(
        protocol,
        "git:"
            | "http:"
            | "git+ssh:"
            | "git+https:"
            | "ssh:"
            | "https:"
            | "github:"
            | "bitbucket:"
            | "gitlab:"
            | "gist:"
            | "sourcehut:"
    )
}

fn correct_url(giturl: &str) -> String {
    let mut corrected = giturl.to_owned();
    let first_at = last_index_before(&corrected, '@', '#');
    let last_colon_before_hash = last_index_before(&corrected, ':', '#');

    if last_colon_before_hash > first_at {
        let colon = last_colon_before_hash.expect("a compared index exists");
        corrected = format!("{}/{}", &corrected[..colon], &corrected[colon + 1..]);
    }

    if last_index_before(&corrected, ':', '#').is_none() && !corrected.contains("//") {
        return format!("git+ssh://{corrected}");
    }
    corrected
}

fn last_index_before(text: &str, needle: char, before: char) -> Option<usize> {
    let end = text.find(before).unwrap_or(text.len());
    text[..end].rfind(needle)
}

pub fn decode_uri_component(value: &str) -> Option<String> {
    if !has_valid_percent_sequences(value) {
        return None;
    }
    percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|value| value.into_owned())
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

fn split_once(text: &JsString, separator: char) -> Option<(JsString, JsString)> {
    let index = text.find(separator)?;
    Some((text.slice(0, index), text.slice(index + 1, text.len())))
}

fn build_git_source(repo: JsString, host: JsString, path: JsString, reference: Option<JsString>) -> Option<GitSource> {
    if path.starts_with('/') {
        return None;
    }
    let mut normalized_path = path.strip_suffix(".git").unwrap_or(path);
    while normalized_path.starts_with('/') {
        normalized_path = normalized_path.slice(1, normalized_path.len());
    }
    if host.is_empty() || normalized_path.is_empty() || normalized_path.split('/').count() < 2 {
        return None;
    }
    if has_unsafe_git_install_part(&host, false) || has_unsafe_git_install_part(&normalized_path, true) {
        return None;
    }
    Some(GitSource {
        source_type: GitSourceType::Git,
        repo,
        host,
        path: normalized_path,
        pinned: reference.is_some(),
        reference,
    })
}

fn decode_validation_text(value: &JsString) -> Option<JsString> {
    let mut output = JsString::default();
    let mut index = 0;
    while index < value.len() {
        if value.units()[index] != b'%' as u16 {
            output.push_unit(value.units()[index]);
            index += 1;
            continue;
        }
        let mut bytes = Vec::new();
        while index < value.len() && value.units()[index] == b'%' as u16 {
            let high = char::from_u32(u32::from(*value.units().get(index + 1)?))?.to_digit(16)?;
            let low = char::from_u32(u32::from(*value.units().get(index + 2)?))?.to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
            index += 3;
        }
        let decoded = match String::from_utf8(bytes) {
            Ok(decoded) => decoded,
            Err(_) => return None,
        };
        output.push_str(decoded);
    }
    Some(output)
}

fn has_unsafe_git_install_part(value: &JsString, allow_slash: bool) -> bool {
    let Some(decoded) = decode_validation_text(value) else {
        return true;
    };
    for candidate in [value, &decoded] {
        if candidate.contains('\0') || candidate.contains('\\') || candidate.starts_with('/') {
            return true;
        }
        if !allow_slash && candidate.contains('/') {
            return true;
        }
        if candidate.split('/').any(|part| part == "..") {
            return true;
        }
    }
    false
}

fn parse_generic_git_url(url: &JsString) -> Option<GitSource> {
    let split = split_ref(url);
    let repo_without_ref = split.repo;
    let mut repo = repo_without_ref.clone();
    let host;
    let path;
    if let Some(rest) = repo_without_ref.strip_prefix("git@")
        && let Some((scp_host, scp_path)) = split_once(&rest, ':')
        && !scp_path.is_empty()
    {
        host = scp_host;
        path = scp_path;
    } else if has_explicit_protocol(&repo_without_ref) {
        let parsed = match Url::parse(&encode_node_utf8(&repo_without_ref)) {
            Ok(parsed) => parsed,
            Err(_) => return None,
        };
        host = parsed.host_str().unwrap_or("").into();
        path = parsed.path().trim_start_matches('/').into();
    } else {
        let slash_index = repo_without_ref.find('/')?;
        host = repo_without_ref.slice(0, slash_index);
        path = repo_without_ref.slice(slash_index + 1, repo_without_ref.len());
        if !host.contains('.') && host != "localhost" {
            return None;
        }
        repo = js_format!("https://{}", repo_without_ref);
    }
    build_git_source(repo, host, path, split.reference)
}
