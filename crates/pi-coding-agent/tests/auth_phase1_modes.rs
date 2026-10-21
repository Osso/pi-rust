//! Private creation modes apply to every newly created auth directory, not only its leaf.
#[cfg(unix)]
#[test]
fn auth_creates_all_parent_directories_private() {
    use pi_coding_agent::core::auth_storage::{AuthStorageBackend, FileAuthStorageBackend};
    use std::{fs, os::unix::fs::PermissionsExt};
    let root = std::env::temp_dir().join(format!("auth-modes-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("one/two/auth.json");
    FileAuthStorageBackend::new(file.to_str().unwrap())
        .with_lock(Box::new(|_| Ok(None)))
        .unwrap();
    assert_eq!(
        fs::metadata(root.join("one")).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join("one/two")).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
}
