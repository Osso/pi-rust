#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::node_fs::{node_errno_name, node_error_code, node_fs_error, node_fs_error_record, node_read_error};
use serde_json::json;
use std::fs;
use std::io;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pi-node-fs-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove isolated filesystem fixture");
    }
}

#[test]
fn real_filesystem_messages_and_own_metadata_match_node() {
    let fixture = Fixture::new();
    let root = &fixture.0;
    let file = root.join("plain ' quoted file");
    fs::write(&file, "fixture").unwrap();
    let loop_path = root.join("loop");
    std::os::unix::fs::symlink("loop", &loop_path).unwrap();
    let mut node = oracle::TsOracle::spawn("node-fs-error-oracle.ts", "packages/coding-agent/package.json");
    let cases = [
        ("write", root.join("missing/child"), "open"),
        ("read", root.join("missing/child"), "open"),
        ("write", root.clone(), "open"),
        ("read", root.clone(), "read"),
        ("write", file.join("child"), "open"),
        ("read", file.join("child"), "open"),
        ("mkdir", file.clone(), "mkdir"),
        ("read", loop_path, "open"),
    ];
    for (operation, path, syscall) in cases {
        let error = match operation {
            "write" => fs::write(&path, "fixture").unwrap_err(),
            "read" => fs::read(&path).unwrap_err(),
            "mkdir" => fs::create_dir(&path).unwrap_err(),
            _ => unreachable!(),
        };
        let path = path.to_str().unwrap();
        let expected = node.request(&json!({"operation":operation,"path":path}));
        let record = node_fs_error_record(&error, syscall, path);
        let mut own =
            json!({"message":record.message,"errno":record.errno,"code":record.code,"syscall":record.syscall});
        if let Some(path) = &record.path {
            own["path"] = json!(path);
        }
        assert_eq!(
            json!({"name":record.name,"message":record.message,"own":own}),
            expected,
            "{operation}: {path}"
        );
        assert_eq!(node_fs_error(&error, syscall, path), record.message);
        if operation == "read" {
            assert_eq!(node_read_error(&error, path), record.message);
        }
    }
}

#[test]
fn constructed_error_kinds_keep_existing_messages_without_invented_errno() {
    for (kind, code, description) in [
        (io::ErrorKind::NotFound, "ENOENT", "no such file or directory"),
        (io::ErrorKind::PermissionDenied, "EACCES", "permission denied"),
        (
            io::ErrorKind::IsADirectory,
            "EISDIR",
            "illegal operation on a directory",
        ),
        (io::ErrorKind::NotADirectory, "ENOTDIR", "not a directory"),
        (io::ErrorKind::AlreadyExists, "EEXIST", "file already exists"),
        (io::ErrorKind::InvalidInput, "EINVAL", "invalid argument"),
        (io::ErrorKind::DirectoryNotEmpty, "ENOTEMPTY", "directory not empty"),
        (io::ErrorKind::Other, "EIO", "custom detail"),
    ] {
        let error = io::Error::new(kind, "custom detail");
        let record = node_fs_error_record(&error, "open", "fixture");
        assert_eq!(record.errno, None);
        assert_eq!(record.path.as_deref(), Some("fixture"));
        assert_eq!(record.message, format!("{code}: {description}, open 'fixture'"));
        assert_eq!(record.code, code);
        assert_eq!(node_error_code(&error), code);
        if kind == io::ErrorKind::IsADirectory {
            assert_eq!(
                node_read_error(&error, "fixture"),
                "EISDIR: illegal operation on a directory, read 'fixture'"
            );
        }
    }
}

#[test]
fn errno_names_and_unproven_description_behavior_remain_unchanged() {
    for (errno, name) in [
        (2, "ENOENT"),
        (13, "EACCES"),
        (20, "ENOTDIR"),
        (21, "EISDIR"),
        (26, "ETXTBSY"),
        (40, "ELOOP"),
    ] {
        assert_eq!(node_errno_name(errno), Some(name));
        assert_eq!(node_error_code(&io::Error::from_raw_os_error(errno)), name);
    }
    for errno in [-1, 4096] {
        assert_eq!(node_errno_name(errno), None);
    }
    let error = io::Error::from_raw_os_error(26);
    assert_eq!(
        node_fs_error(&error, "open", "busy"),
        format!("ETXTBSY: {error}, open 'busy'")
    );
}
