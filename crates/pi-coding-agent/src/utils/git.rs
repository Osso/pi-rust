use percent_encoding::percent_decode_str;
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
    pub repo: String,
    /// Git host domain, for example `github.com`.
    pub host: String,
    /// Repository path, for example `user/repo`.
    pub path: String,
    /// Git ref (branch, tag, commit) when specified.
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
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
pub fn parse_git_url(source: &str) -> Option<GitSource> {
    let trimmed = source.trim();
    let has_git_prefix = trimmed.starts_with("git:");
    let url = if has_git_prefix {
        trimmed.get(4..)?.trim()
    } else {
        trimmed
    };

    if !has_git_prefix && !has_explicit_protocol(url) {
        return None;
    }

    let split = split_ref(url);
    if let Some(source) = parse_hosted_candidates(&split, url) {
        return Some(source);
    }
    if let Some(source) = parse_https_candidates(&split, url) {
        return Some(source);
    }
    parse_generic_git_url(url)
}

fn parse_hosted_candidates(split: &SplitRef, url: &str) -> Option<GitSource> {
    for candidate in reference_candidates(split, url) {
        let Some(info) = hosted_git_info(&candidate) else {
            continue;
        };
        if split.reference.is_some() && info.project.contains('@') {
            continue;
        }
        let repo = if needs_https_prefix(&split.repo) {
            format!("https://{}", split.repo)
        } else {
            split.repo.clone()
        };
        return build_from_hosted_info(repo, info, split);
    }
    None
}

fn parse_https_candidates(split: &SplitRef, url: &str) -> Option<GitSource> {
    for candidate in https_candidates(split, url) {
        let Some(info) = hosted_git_info(&candidate) else {
            continue;
        };
        if split.reference.is_some() && info.project.contains('@') {
            continue;
        }
        return build_from_hosted_info(format!("https://{}", split.repo), info, split);
    }
    None
}

fn reference_candidates(split: &SplitRef, url: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(reference) = &split.reference {
        candidates.push(format!("{}#{reference}", split.repo));
    }
    candidates.push(url.to_owned());
    candidates
}

fn https_candidates(split: &SplitRef, url: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(reference) = &split.reference {
        candidates.push(format!("https://{}#{reference}", split.repo));
    }
    candidates.push(format!("https://{url}"));
    candidates
}

fn build_from_hosted_info(repo: String, info: HostedGitInfo, split: &SplitRef) -> Option<GitSource> {
    let user = info.user.unwrap_or_else(|| "null".to_owned());
    let path = format!("{user}/{}", info.project);
    let reference = info.committish.or_else(|| split.reference.clone());
    build_git_source(repo, info.domain, path, reference)
}

fn has_explicit_protocol(url: &str) -> bool {
    let lowercase = url.to_ascii_lowercase();
    ["http://", "https://", "ssh://", "git://"]
        .iter()
        .any(|prefix| lowercase.starts_with(prefix))
}

fn needs_https_prefix(repo: &str) -> bool {
    !(repo.starts_with("http://")
        || repo.starts_with("https://")
        || repo.starts_with("ssh://")
        || repo.starts_with("git://")
        || repo.starts_with("git@"))
}

struct SplitRef {
    repo: String,
    reference: Option<String>,
}

fn split_ref(url: &str) -> SplitRef {
    if let Some(split) = split_scp_like_ref(url) {
        return split;
    }
    if url.contains("://") {
        return split_url_ref(url);
    }
    split_host_path_ref(url)
}

fn split_scp_like_ref(url: &str) -> Option<SplitRef> {
    let rest = url.strip_prefix("git@")?;
    let (host, path_with_maybe_ref) = rest.split_once(':')?;
    if path_with_maybe_ref.is_empty() {
        return None;
    }
    let (repo_path, reference) = split_reference(path_with_maybe_ref)?;
    Some(SplitRef {
        repo: format!("git@{host}:{repo_path}"),
        reference: Some(reference.to_owned()),
    })
}

fn split_url_ref(url: &str) -> SplitRef {
    let Ok(mut parsed) = Url::parse(url) else {
        return unchanged_ref(url);
    };
    let path_with_maybe_ref = parsed.path().trim_start_matches('/').to_owned();
    let Some((repo_path, reference)) = split_reference(&path_with_maybe_ref) else {
        return unchanged_ref(url);
    };

    parsed.set_path(&format!("/{repo_path}"));
    SplitRef {
        repo: parsed.to_string().trim_end_matches('/').to_owned(),
        reference: Some(reference.to_owned()),
    }
}

fn split_host_path_ref(url: &str) -> SplitRef {
    let Some(slash_index) = url.find('/') else {
        return unchanged_ref(url);
    };
    let host = &url[..slash_index];
    let path_with_maybe_ref = &url[slash_index + 1..];
    let Some((repo_path, reference)) = split_reference(path_with_maybe_ref) else {
        return unchanged_ref(url);
    };
    SplitRef {
        repo: format!("{host}/{repo_path}"),
        reference: Some(reference.to_owned()),
    }
}

fn split_reference(path_with_maybe_ref: &str) -> Option<(&str, &str)> {
    let separator = path_with_maybe_ref.find('@')?;
    let repo_path = &path_with_maybe_ref[..separator];
    let reference = &path_with_maybe_ref[separator + 1..];
    if repo_path.is_empty() || reference.is_empty() {
        return None;
    }
    Some((repo_path, reference))
}

fn unchanged_ref(url: &str) -> SplitRef {
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

fn hosted_git_info(giturl: &str) -> Option<HostedGitInfo> {
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

fn decode_uri_component(value: &str) -> Option<String> {
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

fn build_git_source(repo: String, host: String, path: String, reference: Option<String>) -> Option<GitSource> {
    if path.starts_with('/') {
        return None;
    }
    let normalized_path = path
        .strip_suffix(".git")
        .unwrap_or(&path)
        .trim_start_matches('/')
        .to_owned();
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

fn has_unsafe_git_install_part(value: &str, allow_slash: bool) -> bool {
    let Some(decoded) = decode_uri_component(value) else {
        return true;
    };
    for candidate in [value, decoded.as_str()] {
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

fn parse_generic_git_url(url: &str) -> Option<GitSource> {
    let split = split_ref(url);
    let repo_without_ref = split.repo;
    let mut repo = repo_without_ref.clone();
    let host;
    let path;

    if let Some(rest) = repo_without_ref.strip_prefix("git@")
        && let Some((scp_host, scp_path)) = rest.split_once(':')
        && !scp_path.is_empty()
    {
        host = scp_host.to_owned();
        path = scp_path.to_owned();
    } else if has_explicit_protocol(&repo_without_ref) {
        let parsed = Url::parse(&repo_without_ref).ok()?;
        host = parsed.host_str().unwrap_or("").to_owned();
        path = parsed.path().trim_start_matches('/').to_owned();
    } else {
        let slash_index = repo_without_ref.find('/')?;
        host = repo_without_ref[..slash_index].to_owned();
        path = repo_without_ref[slash_index + 1..].to_owned();
        if !host.contains('.') && host != "localhost" {
            return None;
        }
        repo = format!("https://{repo_without_ref}");
    }

    build_git_source(repo, host, path, split.reference)
}
