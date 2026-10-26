//! Named credential migration and canonical account-local Claude configuration.
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsString, JsValue},
};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum ProfileError {
    File(io::Error),
    Message(&'static str),
}
impl From<io::Error> for ProfileError {
    fn from(error: io::Error) -> Self {
        Self::File(error)
    }
}
type Result<T> = std::result::Result<T, ProfileError>;

pub fn validate_names(names: &[JsString]) -> std::result::Result<(), CallbackError> {
    let mut unique = HashSet::new();
    for name in names {
        let valid = name
            .units()
            .first()
            .is_some_and(|c| *c <= 127 && (*c as u8).is_ascii_alphanumeric())
            && name
                .units()
                .iter()
                .all(|c| *c <= 127 && ((*c as u8).is_ascii_alphanumeric() || *c == 95 || *c == 45));
        if !valid {
            return Err(CallbackError::new(
                "Invalid profile name: use letters, digits, underscores or hyphens",
            ));
        }
        if !unique.insert(name) {
            return Err(CallbackError::new("Duplicate profile name"));
        }
    }
    Ok(())
}

pub fn configured_names(settings: &JsValue) -> std::result::Result<Option<Vec<JsString>>, CallbackError> {
    let Some(names) = settings.get("accountProfiles") else {
        return Ok(None);
    };
    let names = names
        .as_array()
        .ok_or_else(|| CallbackError::new("provider.accountProfiles must be an ordered list of profile names"))?;
    let names = names
        .iter()
        .map(|name| {
            name.as_string()
                .cloned()
                .ok_or_else(|| CallbackError::new("Invalid profile name: use letters, digits, underscores or hyphens"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    validate_names(&names)?;
    Ok(Some(names))
}

fn inspect(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(stat) => Ok(Some(stat)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn restrict_directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(ProfileError::Message(
            "Account profile path must be a directory, not a symlink",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn require_regular(path: &Path) -> Result<()> {
    if inspect(path)?.is_some_and(|s| !s.is_file()) {
        return Err(ProfileError::Message(
            "Credentials must be a regular file, not a symlink",
        ));
    }
    Ok(())
}
fn read_credentials(path: &Path) -> Result<(Vec<u8>, JsValue)> {
    require_regular(path)?;
    let content = fs::read(path)?;
    let text = std::str::from_utf8(&content)
        .map_err(|_| ProfileError::Message("Invalid credentials: expected a nonempty JSON object"))?;
    let value = js_json::parse_lossless(text)
        .map_err(|_| ProfileError::Message("Invalid credentials: expected a nonempty JSON object"))?;
    if !matches!(&value,JsValue::Object(fields) if !fields.is_empty()) {
        return Err(ProfileError::Message(
            "Invalid credentials: expected a nonempty JSON object",
        ));
    }
    let token = value
        .get("claudeAiOauth")
        .and_then(|oauth| oauth.get("accessToken"))
        .and_then(JsValue::as_string);
    if token.is_none_or(|token| token.trim().is_empty()) {
        return Err(ProfileError::Message(
            "Invalid credentials: expected a nonempty OAuth access token",
        ));
    }
    Ok((content, value))
}
fn has_owner(path: &Path) -> Result<bool> {
    let Some(stat) = inspect(path)? else {
        return Ok(false);
    };
    if !stat.is_file() || fs::read(path)? != b"shared-v1\n" {
        return Err(ProfileError::Message("Invalid credential ownership marker"));
    }
    Ok(true)
}
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    require_regular(path)?;
    let parent = path.parent().ok_or(ProfileError::Message("Invalid credential path"))?;
    let temp = tempfile::Builder::new()
        .prefix(".bridge-credentials-")
        .tempdir_in(parent)?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let file = temp.path().join("credentials.json");
    use std::io::Write;
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&file)?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    output.write_all(content)?;
    fs::rename(file, path)?;
    Ok(())
}
fn profile_dir(home: &Path, name: &str) -> PathBuf {
    home.join(".config/pi-claude-bridge/profiles").join(name)
}

fn freshest(home: &Path, name: &str, dir: &Path) -> Result<PathBuf> {
    let original = home.join(".claude");
    let saved = original.join(format!("{name}.credentials.json"));
    let isolated = dir.join(".credentials.json");
    let mut selected = saved.clone();
    let mut newest = None;
    for path in [&isolated, &saved] {
        require_regular(path)?;
        if let Some(stat) = inspect(path)? {
            let time = stat.modified()?;
            if newest.is_none_or(|newest| time > newest) {
                selected = path.clone();
                newest = Some(time);
            }
        }
    }
    let live = original.join(".credentials.json");
    if let Some(stat) = inspect(&live)? {
        let time = stat.modified()?;
        if newest.is_none_or(|newest| time >= newest) {
            let marker = original.join(".active-profile");
            let active = if inspect(&marker)?.is_some() {
                Some(fs::read_to_string(marker)?)
            } else {
                None
            };
            if active.as_deref().is_some_and(|v| v.trim() == name)
                && (selected != isolated || newest.is_none_or(|newest| time > newest))
            {
                selected = live;
            }
        }
    }
    Ok(selected)
}

fn share_entry(original: &Path, shared: &Path, name: &str) -> Result<()> {
    let stat = match inspect(shared)? {
        Some(stat) => stat,
        None => match symlink(original, shared) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => fs::symlink_metadata(shared)?,
            Err(error) => return Err(error.into()),
        },
    };
    let original_stat = inspect(original)?;
    if ["policy-limits.json", "remote-settings.json"].contains(&name)
        && stat.is_file()
        && original_stat.as_ref().is_some_and(fs::Metadata::is_file)
        && fs::read(original)? == fs::read(shared)?
    {
        let parent = shared.parent().ok_or(ProfileError::Message("Invalid config path"))?;
        let temp = tempfile::Builder::new().prefix(".bridge-config-").tempdir_in(parent)?;
        let link = temp.path().join("shared");
        symlink(original, &link)?;
        fs::rename(link, shared)?;
        return Ok(());
    }
    if ["skills", "plugins", "cache"].contains(&name)
        && stat.is_dir()
        && original_stat.as_ref().is_some_and(fs::Metadata::is_dir)
    {
        return Ok(());
    }
    if name == ".claude.json" && stat.is_file() && original_stat.as_ref().is_some_and(fs::Metadata::is_file) {
        return Ok(());
    }
    if !stat.is_symlink() {
        return Err(ProfileError::Message(
            "Account profile config path conflicts with the original config entry",
        ));
    }
    let target = fs::read_link(shared)?;
    let resolved = if target.is_absolute() {
        target
    } else {
        shared
            .parent()
            .ok_or(ProfileError::Message("Invalid config path"))?
            .join(target)
    };
    let mut normalized = PathBuf::new();
    for component in resolved.components() {
        if component == std::path::Component::ParentDir {
            normalized.pop();
        } else {
            normalized.push(component);
        }
    }
    if normalized != original {
        return Err(ProfileError::Message(
            "Account profile config path conflicts with the original config entry",
        ));
    }
    Ok(())
}
fn share_config(home: &Path, dir: &Path) -> Result<()> {
    let original = home.join(".claude");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(original.join("projects"))?;
    for entry in fs::read_dir(&original)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ProfileError::Message("Invalid config entry"))?;
        if [
            ".credentials.json",
            ".active-profile",
            "policy-limits.json.stamp.json",
            "policy-limits.json.signature.json",
            "policy-limits.json.signature-iat.json",
            "remote-settings.json.signature.json",
            "remote-settings.json.signature-iat.json",
        ]
        .contains(&name.as_str())
            || name.ends_with(".credentials.json")
        {
            continue;
        }
        share_entry(&entry.path(), &dir.join(&name), &name)?;
    }
    Ok(())
}
fn migrate(home: &Path, name: &str, dir: &Path) -> Result<()> {
    let owner = dir.join(".credential-owner");
    let owned = has_owner(&owner)?;
    let lock = dir.join(".credential-migration");
    if inspect(&lock)?.is_some() {
        return Err(ProfileError::Message("migration contention"));
    }
    if owned {
        return Ok(());
    }
    match fs::DirBuilder::new().mode(0o700).create(&lock) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(ProfileError::Message("migration contention"));
        }
        Err(e) => return Err(e.into()),
    }
    let result = (|| {
        if has_owner(&owner)? {
            return Ok(());
        }
        let source = freshest(home, name, dir)?;
        let (content, _) = read_credentials(&source)?;
        atomic_write(&dir.join(".credentials.json"), &content)?;
        atomic_write(&owner, b"shared-v1\n")
    })();
    let cleanup = fs::remove_dir_all(lock).map_err(ProfileError::from);
    result.and(cleanup)
}
fn report<T>(operation: &str, name: &str, result: Result<T>) -> std::result::Result<T, CallbackError> {
    result.map_err(|error| match error {
        ProfileError::File(error) => CallbackError::new(format!(
            "Cannot {operation} credentials for profile {name}: {}",
            pi_js_compat::node_fs::node_error_code(&error)
        )),
        ProfileError::Message("migration contention") => {
            CallbackError::new(format!("Credential migration already in progress for profile {name}"))
        }
        ProfileError::Message(message) => CallbackError::new(message),
    })
}
fn validate_home_name(home: &Path, name: &str) -> std::result::Result<(), CallbackError> {
    validate_names(&[name.into()])?;
    if !home.is_absolute() {
        return Err(CallbackError::new(
            "Account profiles require an absolute home directory",
        ));
    }
    Ok(())
}
pub fn prepare_profile(home: &Path, name: &str) -> std::result::Result<PathBuf, CallbackError> {
    validate_home_name(home, name)?;
    let dir = profile_dir(home, name);
    report(
        "read/prepare",
        name,
        (|| {
            for path in [
                home.join(".config/pi-claude-bridge"),
                home.join(".config/pi-claude-bridge/profiles"),
                dir.clone(),
            ] {
                restrict_directory(&path)?;
            }
            share_config(home, &dir)?;
            migrate(home, name, &dir)?;
            read_credentials(&dir.join(".credentials.json"))?;
            fs::set_permissions(dir.join(".credentials.json"), fs::Permissions::from_mode(0o600))?;
            Ok(dir)
        })(),
    )
}
pub fn read_active_profile(
    home: &Path,
    config_dir: Option<&Path>,
) -> std::result::Result<Option<String>, CallbackError> {
    let dir = config_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".claude"));
    match fs::read_to_string(dir.join(".active-profile")) {
        Ok(text) => Ok(Some(text.trim().into())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CallbackError::new("Cannot read Claude active profile marker")),
    }
}
pub fn read_usage_credentials(
    home: &Path,
    profile: Option<&str>,
    config_dir: Option<&Path>,
) -> std::result::Result<JsValue, CallbackError> {
    let read = || -> Result<JsValue> {
        let source = if let Some(name) = profile {
            validate_home_name(home, name).map_err(|_| ProfileError::Message("Invalid profile"))?;
            let dir = profile_dir(home, name);
            if has_owner(&dir.join(".credential-owner"))? {
                dir.join(".credentials.json")
            } else {
                freshest(home, name, &dir)?
            }
        } else {
            config_dir
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".claude"))
                .join(".credentials.json")
        };
        Ok(read_credentials(&source)?.1)
    };
    read().map_err(|error| match error {
        ProfileError::File(e) if e.kind() == io::ErrorKind::NotFound => {
            CallbackError::new("Missing Claude credentials for current account")
        }
        _ => CallbackError::new("Invalid or unreadable Claude credentials for current account"),
    })
}
