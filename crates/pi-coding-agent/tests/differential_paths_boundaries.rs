use crate::shared_oracle as oracle;

use pi_coding_agent::utils::paths::{PathInputOptions, normalize_path};
use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn file_url_errors_preserve_frozen_names_messages_and_properties() {
    // Frozen utils/paths.ts:76-77 passes file URLs directly to Node fileURLToPath.
    let inputs = [
        "file:///tmp/a%2Fb",
        "file://host/tmp/x",
        "file://[invalid]/x",
        "file:///%",
        "file:///%C0%AF",
        "file:///%ED%A0%80",
        "file:///%00",
        "file:///a%5Cb",
        "file:///tmp/a?query#fragment",
        "file://localhost/tmp/x",
        "file:///tmp/../x",
        "file://user:password@host/x",
        "file://host:80/x",
        "file:////host/x",
        "file://///a/../b",
        "file:////a/../../b",
        "file:////a/..//b",
        "file://///",
        "file:///C:/..//",
        "file:///C|/../../x",
        "file://C:/x",
        "file:////C:/../../x",
        "file:////%2e%2e/a",
        "file://localhost//a",
        "file://localhost///a/../b",
        "file:///\\\\a\\\\b",
        "file:////a\t/b\n",
        "file:////a?//query",
        "file:////a#//fragment",
    ];
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let cases = inputs
        .iter()
        .map(|input| pi_js_compat::js_json!({"kind":"normalize","input":input}))
        .collect();
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":JsValue::Array(cases)}));
    let actual: Vec<_> = inputs
        .iter()
        .map(|input| match normalize_path(*input, &PathInputOptions::default()) {
            Ok(value) => pi_js_compat::js_json!({"ok":true,"value":value}),
            Err(error) => {
                pi_js_compat::js_json!({"ok":false,"error":error.native_error().expect("Node error identity").value()})
            }
        })
        .collect();
    for ((input, actual), expected) in inputs
        .iter()
        .zip(actual)
        .zip(expected["output"]["results"].as_array().unwrap())
    {
        assert_eq!(&actual, expected, "URL {input}");
    }
}

#[test]
fn canonical_paths_follow_real_files_symlink_chains_and_source_catch_boundaries() {
    use pi_coding_agent::utils::paths::canonicalize_path;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::create_dir(root.join("dir")).unwrap();
    std::fs::write(root.join("dir/file"), b"fixture").unwrap();
    symlink("dir", root.join("relative")).unwrap();
    symlink("relative", root.join("chain")).unwrap();
    symlink(root.join("dir"), root.join("absolute")).unwrap();
    symlink("absent", root.join("dangling")).unwrap();
    symlink("cycle-b", root.join("cycle-a")).unwrap();
    symlink("cycle-a", root.join("cycle-b")).unwrap();
    let raw = std::ffi::OsString::from_vec(vec![b'r', 0xff]);
    std::fs::write(root.join(&raw), b"raw bytes").unwrap();
    symlink(&raw, root.join("raw-link")).unwrap();
    let socket = std::os::unix::net::UnixListener::bind(root.join("socket")).unwrap();
    symlink("socket", root.join("socket-link")).unwrap();
    let names = [
        "",
        "dir",
        "dir/file",
        "relative/file",
        "chain/file",
        "absolute/file",
        "dir/../chain/file",
        "missing",
        "dangling",
        "cycle-a",
        "dir/file/child",
        "raw-link",
        "socket",
        "socket-link",
        "socket-link/child",
    ];
    let mut inputs: Vec<JsString> = names
        .iter()
        .map(|name| JsString::from(root.join(name).to_str().unwrap()))
        .collect();
    inputs.extend([JsString::from(""), JsString::from("/"), JsString::from("bad\0path")]);
    let cases = inputs
        .iter()
        .map(|input| pi_js_compat::js_json!({"kind":"canonicalize","name":"realpath","path":input}))
        .collect();
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-paths-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":JsValue::Array(cases)}));
    for (input, expected) in inputs.iter().zip(expected["output"]["results"].as_array().unwrap()) {
        assert_eq!(
            JsValue::String(canonicalize_path(input)),
            expected["value"],
            "realpath {input:?}"
        );
    }
    drop(socket);
}

#[test]
fn file_url_adversarial_host_path_and_percent_matrix() {
    let hosts = [
        "",
        "localhost",
        "LOCALHOST",
        "local%68ost",
        "localhost.",
        "127.0.0.1",
        "0x7f000001",
        "[::1]",
        "[invalid]",
        "server",
        "user@localhost",
        "localhost:80",
        "%",
        "%00",
        "%2f",
        "a b",
        "a^b",
        "a|b",
        "a<b",
        "é",
        "\u{200d}",
        "\u{ff0f}",
        "C:",
        "C|",
    ];
    let paths = [
        "",
        "/",
        "//",
        "///",
        "/a",
        "//a",
        "///a",
        "/a//b",
        "/a/../b",
        "//a/../../b",
        "/./",
        "/../",
        "//../",
        "/%2e/x",
        "/.%2E/x",
        "/%2e./x",
        "/%2E%2e/x",
        "/a/%252e/x",
        "/C:",
        "/C|",
        "/C:/",
        "/C|/",
        "/C:/..//",
        "//C:/../../x",
        "/x/C:/../../x",
        "/%43:/..",
        "/C%3A/..",
        "/C%7C/..",
        "/a%3Ab%7Cc",
        "/%253A/%257C",
        "/%2f",
        "/%2F/%",
        "/é/%2f?é'`<>#é`<> ",
        "/a b/{x}/%2f?query#fragment",
        "/%5c",
        "/%",
        "/%0",
        "/%GG",
        "/%C0%AF",
        "/%ED%A0%80",
        "/%F4%90%80%80",
        "/%E2%82%AC",
        "/%00x",
        "/é/中文",
        "/a\0b",
        "/a ",
        "/a\tb\rc\nd",
        "\\a\\..\\b",
        "//a?query/../x",
        "//a#fragment/../x",
        "?query",
        "#fragment",
    ];
    let mut inputs: Vec<JsString> = hosts
        .iter()
        .flat_map(|host| {
            paths
                .iter()
                .map(move |path| JsString::from(format!("file://{host}{path}")))
        })
        .collect();
    inputs.extend([
        JsString::from("file:///raw/") + JsString::from_utf16(vec![0xd800]),
        JsString::from("file://") + JsString::from_utf16(vec![0xdfff]) + "/x",
    ]);
    compare_normalized_urls(&inputs);
}

fn compare_normalized_urls(inputs: &[JsString]) {
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let mut mismatches = Vec::new();
    for batch in inputs.chunks(100) {
        let cases = batch
            .iter()
            .map(|input| pi_js_compat::js_json!({"kind":"normalize","input":input}))
            .collect();
        let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":JsValue::Array(cases)}));
        for (input, expected) in batch.iter().zip(expected["output"]["results"].as_array().unwrap()) {
            let actual = match normalize_path(input, &PathInputOptions::default()) {
                Ok(value) => pi_js_compat::js_json!({"ok":true,"value":value}),
                Err(error) => {
                    pi_js_compat::js_json!({"ok":false,"error":error.native_error().expect("Node error identity").value()})
                }
            };
            if &actual != expected {
                mismatches.push(format!(
                    "{input:?}: native={}, Node={}",
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(expected)
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches / {} cases:\n{}",
        mismatches.len(),
        inputs.len(),
        mismatches.join("\n")
    );
}

#[test]
fn cloud_sync_real_process_argv_stdio_and_ignored_results_match_source() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/usr/bin/node", root.path().join("node")).unwrap();
    let executable = root.path().join("setfattr");
    let log = root.path().join("argv.jsonl");
    let script = "#!/usr/bin/python3\nimport json,os,sys\nwith open(os.environ['PATHS_ARGV_LOG'], 'a') as log:\n log.write(json.dumps({'argv':sys.argv[1:], 'stdin':sys.stdin.read()})+'\\n')\nprint('must not escape stdout')\nprint('must not escape stderr',file=sys.stderr)\nsys.exit(23)\n";
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let input = JsString::from("./literal space/@~") + JsString::from_utf16(vec![0xd800]);
    compare_cloud_sync_processes(root.path(), &log, &input);
    let histories: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(histories.len(), 2);
    assert_eq!(histories[0], histories[1]);
    assert_eq!(
        histories[0],
        serde_json::json!({"argv":["-n","user.com.dropbox.ignored","-v","1","./literal space/@~�"],"stdin":""})
    );
    std::fs::remove_file(&executable).unwrap();
    compare_cloud_sync_processes(root.path(), &log, &input);
    std::fs::write(&executable, "not an executable image").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    compare_cloud_sync_processes(root.path(), &log, &input);
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 2);
}

fn compare_cloud_sync_processes(directory: &std::path::Path, log: &std::path::Path, input: &JsString) {
    use std::process::Command;
    let mut source = oracle::TsOracle::spawn_with_environment(
        "paths-boundaries-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
        &[
            ("PATH", directory.as_os_str().to_owned()),
            ("PATHS_ARGV_LOG", log.as_os_str().to_owned()),
        ],
    );
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":[{"kind":"mark","input":input}]}));
    assert_eq!(
        expected["output"]["results"].as_array().unwrap()[0],
        pi_js_compat::js_json!({"ok":true,"value":null})
    );
    let native = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "cloud_sync_process_child", "--nocapture"])
        .env("PATH", directory)
        .env("PATHS_ARGV_LOG", log)
        .env(
            "PATHS_INPUT",
            pi_js_compat::js_json::stringify_lossless(&JsValue::String(input.clone())),
        )
        .output()
        .unwrap();
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert!(!String::from_utf8_lossy(&native.stdout).contains("must not escape"));
    assert!(!String::from_utf8_lossy(&native.stderr).contains("must not escape"));
}

#[test]
#[ignore = "invoked explicitly by cloud-sync process differential"]
fn cloud_sync_process_child() {
    let input = pi_js_compat::js_json::parse_lossless(std::env::var("PATHS_INPUT").unwrap()).unwrap();
    pi_coding_agent::utils::paths::mark_path_ignored_by_cloud_sync(input.as_string().unwrap()).unwrap();
}

#[test]
fn cloud_sync_marking_does_not_suppress_synchronous_argument_errors() {
    // Frozen paths.ts:114-115 / child-process.ts:31-34: spawnSync validation throws.
    let input = JsString::from("bad\0path");
    let mut source = oracle::TsOracle::spawn("paths-boundaries-oracle.ts", "packages/coding-agent/src/utils/paths.ts");
    let expected = source.request(&pi_js_compat::js_json!({"id":1,"cases":[{"kind":"mark","input":input}]}));
    assert_eq!(
        expected["output"]["results"].as_array().unwrap()[0]["ok"],
        JsValue::Bool(false)
    );
    let error = pi_coding_agent::utils::paths::mark_path_ignored_by_cloud_sync(&input).unwrap_err();
    let actual = pi_js_compat::js_json!({"ok":false,"error":error.value()});
    assert_eq!(actual, expected["output"]["results"].as_array().unwrap()[0]);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
