//! Node.js filesystem error text for the built-in tools.
//!
//! The TypeScript tools surface `fs` errors verbatim (`ENOENT: no such file or
//! directory, open '/path'`), so every Rust tool maps `io::Error` through this
//! single formatter.

use std::io;

/// Node `error.code` for an `io::Error`.
pub fn node_error_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "ENOENT",
        io::ErrorKind::PermissionDenied => "EACCES",
        io::ErrorKind::IsADirectory => "EISDIR",
        io::ErrorKind::NotADirectory => "ENOTDIR",
        io::ErrorKind::AlreadyExists => "EEXIST",
        io::ErrorKind::InvalidInput => "EINVAL",
        io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY",
        _ => "EIO",
    }
}

/// Node filesystem error message: `<CODE>: <description>, <syscall> '<path>'`.
pub fn node_fs_error(error: &io::Error, syscall: &str, path: &str) -> String {
    let description = match error.kind() {
        io::ErrorKind::NotFound => "no such file or directory",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::IsADirectory => "illegal operation on a directory",
        io::ErrorKind::NotADirectory => "not a directory",
        io::ErrorKind::AlreadyExists => "file already exists",
        io::ErrorKind::InvalidInput => "invalid argument",
        io::ErrorKind::DirectoryNotEmpty => "directory not empty",
        _ => return format!("{}: {error}, {syscall} '{path}'", node_error_code(error)),
    };
    format!("{}: {description}, {syscall} '{path}'", node_error_code(error))
}
