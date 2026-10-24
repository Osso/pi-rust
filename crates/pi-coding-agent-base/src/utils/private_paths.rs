//! Private socket paths: `mkdirSync(..., {mode: 0o700, recursive: true})`,
//! `chmodSync(socket, 0o600)`, and exit-time socket removal.

use std::path::Path;

pub(crate) fn create_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        if current.is_dir() {
            continue;
        }
        match std::fs::create_dir(&current) {
            Ok(()) => std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(crate) fn set_private_socket_mode(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())
}

pub(crate) fn remove_socket_file_if_present(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}
