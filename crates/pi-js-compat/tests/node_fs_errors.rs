use crate::shared_oracle_2 as oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::node_fs::{
    NodeReadApi, node_errno_name, node_error_code, node_fs_dest_error, node_fs_error, node_fs_error_record,
    node_promises_read_error, node_read_error, node_read_error_record,
};
use serde_json::json;
use std::fs;
use std::io;
use std::path::PathBuf;

#[test]
fn pathless_close_and_eio_messages_match_nodes_uv_exception() {
    for (syscall, path) in [("close", ""), ("open", "fixture")] {
        let input = json!({"errno":-5,"syscall":syscall,"path":path});
        let output=std::process::Command::new("node").args(["--expose-internals","-e",r#"const p=JSON.parse(process.argv[1]);const {UVException}=require('internal/errors');const ctx={errno:p.errno,syscall:p.syscall};if(p.path)ctx.path=p.path;const e=new UVException(ctx);console.log(JSON.stringify({message:e.message,path:e.path??null}));"#]).arg(input.to_string()).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let record = node_fs_error_record(&io::Error::from_raw_os_error(5), syscall, &path.into());
        assert_eq!(json!({"message":record.message,"path":record.path}), expected);
    }
}

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
        ("readFd", root.clone(), "read"),
        ("readAsync", root.clone(), "read"),
        ("readAsync", root.join("missing/child"), "open"),
        ("readAsync", file.join("child"), "open"),
        ("write", file.join("child"), "open"),
        ("read", file.join("child"), "open"),
        ("mkdir", file.clone(), "mkdir"),
        ("read", loop_path, "open"),
    ];
    for (operation, path, syscall) in cases {
        let error = match operation {
            "write" => fs::write(&path, "fixture").unwrap_err(),
            "read" | "readFd" | "readAsync" => fs::read(&path).unwrap_err(),
            "mkdir" => fs::create_dir(&path).unwrap_err(),
            _ => unreachable!(),
        };
        let path = JsString::from(path.to_str().unwrap());
        let expected = node.request(&json!({"operation":operation,"path":path}));
        let record = if operation == "readAsync" {
            node_read_error_record(&error, &path, NodeReadApi::PromisesReadFile)
        } else {
            node_fs_error_record(&error, syscall, &path)
        };
        let mut own =
            json!({"message":record.message,"errno":record.errno,"code":record.code,"syscall":record.syscall});
        if let Some(path) = &record.path {
            own["path"] = json!(path);
        }
        assert_eq!(
            json!({"name":record.name,"message":record.message,"own":own}),
            expected,
            "{operation}: {path:?}"
        );
        if operation == "readAsync" {
            assert_eq!(node_promises_read_error(&error, &path), record.message);
        } else {
            assert_eq!(node_fs_error(&error, syscall, &path), record.message);
        }
        if operation == "read" {
            assert_eq!(node_read_error(&error, &path), record.message);
        }
    }
}

#[test]
fn console_messages_match_node_inspection() {
    let cases = [
        ("plain", None),
        ("single'quote", None),
        ("both'\"quotes", None),
        ("all'\"`quotes\n\t\u{0}\u{1b}", None),
        ("cargo", Some(vec!["test".into(), "-p".into(), "pi-evals".into()])),
        ("cargo", Some(vec!["x".into(); 101])),
    ];
    for (path, args) in cases {
        let record = node_fs_error_record(&io::Error::from_raw_os_error(2), "mkdir", &path.into());
        let input = json!({"name":record.name,"message":record.message,"errno":record.errno,
            "code":record.code,"syscall":record.syscall,"path":record.path,"args":args});
        let node = std::process::Command::new("node")
            .args(["-e", r#"const p=JSON.parse(process.argv[1]); const e=Object.assign(new Error(p.message), {errno:p.errno,code:p.code,syscall:p.syscall,path:p.path}); e.stack=`${p.name}: ${p.message}\n    at fixture`; if(p.args!==null)e.spawnargs=p.args; console.log(require('node:util').inspect(e).replace('\n    at fixture',''));"#])
            .arg(input.to_string())
            .output().unwrap();
        assert!(node.status.success());
        assert_eq!(
            record.console_message(args.as_deref()) + "\n",
            JsString::from(String::from_utf8(node.stdout).unwrap())
        );
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
        let record = node_fs_error_record(&error, "open", &"fixture".into());
        assert_eq!(record.errno, None);
        assert_eq!(record.path.as_ref(), Some(&JsString::from("fixture")));
        assert_eq!(
            record.message,
            JsString::from(format!("{code}: {description}, open 'fixture'"))
        );
        assert_eq!(record.code, code);
        assert_eq!(node_error_code(&error), code);
        if kind == io::ErrorKind::IsADirectory {
            assert_eq!(
                node_read_error(&error, &"fixture".into()),
                "EISDIR: illegal operation on a directory, read 'fixture'"
            );
        }
    }
}

#[test]
fn errno_names_and_text_file_busy_description_match_libuv() {
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
        node_fs_error(&error, "open", &"busy".into()),
        JsString::from("ETXTBSY: text file is busy, open 'busy'")
    );
}

#[test]
fn removed_current_directory_error_has_node_message_and_pathless_own_fields() {
    let error = io::Error::from_raw_os_error(2);
    let record = node_fs_error_record(&error, "uv_cwd", &JsString::default());
    assert_eq!(record.name, "Error");
    assert_eq!(record.errno, Some(-2));
    assert_eq!(record.code, "ENOENT");
    assert_eq!(record.syscall, "uv_cwd");
    assert_eq!(record.path, None);
    assert_eq!(
        record.message,
        "ENOENT: process.cwd failed with error no such file or directory, the current working directory was likely removed without changing the working directory, uv_cwd"
    );
}

#[test]
fn actual_vertical_tab_filesystem_error_console_matches_node() {
    let directory = std::env::temp_dir().join(format!("pi-node-vt-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let fixture = Fixture(directory);
    let file = fixture.0.join("vertical\u{b}tab");
    fs::write(&file, "fixture").unwrap();
    let error = fs::create_dir(&file).unwrap_err();
    let record = node_fs_error_record(&error, "mkdir", &file.to_str().unwrap().into());
    let node = std::process::Command::new("node")
        .args(["-e", r#"const fs=require('node:fs');const util=require('node:util');try{fs.mkdirSync(process.argv[1]);process.exitCode=1;}catch(e){e.stack=`${e.name}: ${e.message}\n    at fixture`;console.log(util.inspect(e).replace('\n    at fixture',''));}"#])
        .arg(&file).output().unwrap();
    assert!(node.status.success());
    assert_eq!(
        record.console_message(None) + "\n",
        JsString::from(String::from_utf8(node.stdout).unwrap())
    );
}

#[test]
fn two_path_rename_and_copyfile_messages_match_node() {
    let root = std::env::temp_dir().join(format!("pi-node-fs-dest-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("missing ' source");
    let dest = root.join("dest");
    for syscall in ["rename", "copyfile"] {
        let error = match syscall {
            "rename" => fs::rename(&source, &dest).unwrap_err(),
            _ => fs::copy(&source, &dest).map(|_| ()).unwrap_err(),
        };
        let script = r#"const fs=require('fs');const [op,s,d]=process.argv.slice(1);try{op==='rename'?fs.renameSync(s,d):fs.copyFileSync(s,d)}catch(e){console.log(JSON.stringify(e.message))}"#;
        let output = std::process::Command::new("node")
            .args(["-e", script, syscall, source.to_str().unwrap(), dest.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected: String = serde_json::from_slice(&output.stdout).unwrap();
        let actual = node_fs_dest_error(
            &error,
            syscall,
            &JsString::from(source.to_str().unwrap()),
            &JsString::from(dest.to_str().unwrap()),
        );
        assert_eq!(actual.to_utf8().unwrap(), expected);
    }
    fs::remove_dir_all(&root).unwrap();
}
