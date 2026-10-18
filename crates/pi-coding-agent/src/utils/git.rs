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

    let mut hosted_candidates = Vec::new();
    if let Some(reference) = &split.reference {
        hosted_candidates.push(format!("{}#{reference}", split.repo));
    }
    hosted_candidates.push(url.to_owned());

    for candidate in hosted_candidates {
        if let Some(info) = hosted_git_info(&candidate) {
            if split.reference.is_some() && info.project.contains('@') {
                continue;
            }
            let repo = if needs_https_prefix(&split.repo) {
                format!("https://{}", split.repo)
            } else {
                split.repo.clone()
            };
            return build_git_source(
                repo,
                info.domain,
                format!("{}/{}", info.user.unwrap_or_else(|| "null".to_owned()), info.project),
                info.committish.or_else(|| split.reference.clone()),
            );
        }
    }

    let mut https_candidates = Vec::new();
    if let Some(reference) = &split.reference {
        https_candidates.push(format!("https://{}#{reference}", split.repo));
    }
    https_candidates.push(format!("https://{url}"));

    for candidate in https_candidates {
        if let Some(info) = hosted_git_info(&candidate) {
            if split.reference.is_some() && info.project.contains('@') {
                continue;
            }
            return build_git_source(
                format!("https://{}", split.repo),
                info.domain,
                format!("{}/{}", info.user.unwrap_or_else(|| "null".to_owned()), info.project),
                info.committish.or_else(|| split.reference.clone()),
            );
        }
    }

    parse_generic_git_url(url)
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
    if let Some(rest) = url.strip_prefix("git@")
        && let Some((host, path_with_maybe_ref)) = rest.split_once(':')
        && !path_with_maybe_ref.is_empty()
        && let Some(reference_separator) = path_with_maybe_ref.find('@')
    {
        let repo_path = &path_with_maybe_ref[..reference_separator];
        let reference = &path_with_maybe_ref[reference_separator + 1..];
        if !repo_path.is_empty() && !reference.is_empty() {
            return SplitRef {
                repo: format!("git@{host}:{repo_path}"),
                reference: Some(reference.to_owned()),
            };
        }
    }

    if url.contains("://") {
        if let Ok(mut parsed) = Url::parse(url) {
            let path_with_maybe_ref = parsed.path().trim_start_matches('/').to_owned();
            if let Some(reference_separator) = path_with_maybe_ref.find('@') {
                let repo_path = &path_with_maybe_ref[..reference_separator];
                let reference = &path_with_maybe_ref[reference_separator + 1..];
                if !repo_path.is_empty() && !reference.is_empty() {
                    parsed.set_path(&format!("/{repo_path}"));
                    return SplitRef {
                        repo: parsed.to_string().trim_end_matches('/').to_owned(),
                        reference: Some(reference.to_owned()),
                    };
                }
            }
        }
        return SplitRef {
            repo: url.to_owned(),
            reference: None,
        };
    }

    let Some(slash_index) = url.find('/') else {
        return SplitRef {
            repo: url.to_owned(),
            reference: None,
        };
    };
    let host = &url[..slash_index];
    let path_with_maybe_ref = &url[slash_index + 1..];
    let Some(reference_separator) = path_with_maybe_ref.find('@') else {
        return SplitRef {
            repo: url.to_owned(),
            reference: None,
        };
    };
    let repo_path = &path_with_maybe_ref[..reference_separator];
    let reference = &path_with_maybe_ref[reference_separator + 1..];
    if repo_path.is_empty() || reference.is_empty() {
        return SplitRef {
            repo: url.to_owned(),
            reference: None,
        };
    }
    SplitRef {
        repo: format!("{host}/{repo_path}"),
        reference: Some(reference.to_owned()),
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

fn extract_host(host: Host, parsed: &Url) -> Option<(Option<String>, String, Option<String>)> {
    let pathname = parsed.path();
    let hash = parsed.fragment().unwrap_or("");

    match host {
        Host::Github => {
            let segments = split_limited(pathname, 5);
            let user = segments.get(1).copied().unwrap_or("");
            let mut project = segments.get(2).copied().unwrap_or("");
            let kind = segments.get(3).copied().filter(|value| !value.is_empty());
            let mut committish = segments.get(4).copied().unwrap_or("");
            if let Some(kind) = kind
                && kind != "tree"
            {
                return None;
            }
            if kind.is_none() {
                committish = hash;
            }
            project = project.strip_suffix(".git").unwrap_or(project);
            if user.is_empty() || project.is_empty() {
                return None;
            }
            Some((
                Some(decode_uri_component(user)?),
                decode_uri_component(project)?,
                Some(decode_uri_component(committish)?),
            ))
        }
        Host::Bitbucket => {
            let segments = split_limited(pathname, 4);
            let user = segments.get(1).copied().unwrap_or("");
            let mut project = segments.get(2).copied().unwrap_or("");
            if segments.get(3).copied() == Some("get") {
                return None;
            }
            project = project.strip_suffix(".git").unwrap_or(project);
            if user.is_empty() || project.is_empty() {
                return None;
            }
            Some((
                Some(decode_uri_component(user)?),
                decode_uri_component(project)?,
                Some(decode_uri_component(hash)?),
            ))
        }
        Host::Gitlab => {
            let path = pathname.strip_prefix('/').unwrap_or(pathname);
            if path.contains("/-/") || path.contains("/archive.tar.gz") {
                return None;
            }
            let mut segments = path.split('/').collect::<Vec<_>>();
            let mut project = segments.pop()?;
            project = project.strip_suffix(".git").unwrap_or(project);
            let user = segments.join("/");
            if user.is_empty() || project.is_empty() {
                return None;
            }
            Some((
                Some(decode_uri_component(&user)?),
                decode_uri_component(project)?,
                Some(decode_uri_component(hash)?),
            ))
        }
        Host::Gist => {
            let segments = split_limited(pathname, 4);
            let mut user = segments.get(1).copied();
            let mut project = segments.get(2).copied();
            if segments.get(3).copied() == Some("raw") {
                return None;
            }
            if project.is_none() {
                let value = user?;
                project = Some(value);
                user = None;
            }
            let project = project?;
            let project = project.strip_suffix(".git").unwrap_or(project);
            Some((
                match user {
                    Some(value) => Some(decode_uri_component(value)?),
                    None => None,
                },
                decode_uri_component(project)?,
                Some(decode_uri_component(hash)?),
            ))
        }
        Host::Sourcehut => {
            let segments = split_limited(pathname, 4);
            let user = segments.get(1).copied().unwrap_or("");
            let mut project = segments.get(2).copied().unwrap_or("");
            if segments.get(3).copied() == Some("archive") {
                return None;
            }
            project = project.strip_suffix(".git").unwrap_or(project);
            if user.is_empty() || project.is_empty() {
                return None;
            }
            Some((
                Some(decode_uri_component(user)?),
                decode_uri_component(project)?,
                Some(decode_uri_component(hash)?),
            ))
        }
    }
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
