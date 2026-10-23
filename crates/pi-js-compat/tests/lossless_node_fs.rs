use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_fs::{
    NodeFsError, NodeReadApi, node_fs_error_record, node_path_argument_error, node_read_error_record,
    node_spawn_error_record, node_unix_connect_error, node_unix_listen_error,
};
use std::io;

fn probe(api: &str, path: &JsString) -> JsValue {
    let input = pi_js_compat::js_json!({"api":api,"path":path});
    let script = r#"const p=JSON.parse(process.argv[1]);const fs=require('node:fs');const cp=require('node:child_process');const net=require('node:net');const result=e=>console.log(JSON.stringify({name:e.name,code:e.code,message:e.message,syscall:e.syscall,path:e.path}));(async()=>{try{switch(p.api){case 'sync':fs.readFileSync(p.path);break;case 'promises':await fs.promises.readFile(p.path);break;case 'nul':fs.readFileSync(p.path);break;case 'spawn':return cp.spawn(p.path,[]).on('error',result);case 'connect':return net.connect(p.path).on('error',e=>console.log(JSON.stringify({message:e.message})));case 'listen':return net.createServer().on('error',e=>console.log(JSON.stringify({message:e.message}))).listen(p.path);}}catch(e){result(e)}})();"#;
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .arg(js_json::stringify_lossless(&input))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    js_json::parse_lossless(String::from_utf8(output.stdout).unwrap()).unwrap()
}
fn raw_path() -> JsString {
    JsString::from(format!("/tmp/pi-nodefs-missing-{}/", std::process::id()))
        + JsString::from_utf16(vec![0xd800])
        + "/child"
}
fn record_value(record: NodeFsError) -> JsValue {
    pi_js_compat::js_json!({"name":record.name,"code":record.code,"message":record.message,"syscall":record.syscall,"path":record.path})
}

#[test]
fn fs_sync_repairs_only_the_crossed_filesystem_path() {
    let path = raw_path();
    assert_eq!(
        record_value(node_fs_error_record(&io::Error::from_raw_os_error(2), "open", &path)),
        probe("sync", &path)
    );
}
#[test]
fn fs_promises_repairs_only_the_crossed_filesystem_path() {
    let path = raw_path();
    assert_eq!(
        record_value(node_read_error_record(
            &io::Error::from_raw_os_error(2),
            &path,
            NodeReadApi::PromisesReadFile
        )),
        probe("promises", &path)
    );
}
#[test]
fn spawn_keeps_raw_executable_in_message_syscall_and_path() {
    let path = raw_path();
    assert_eq!(
        record_value(node_spawn_error_record(&io::Error::from_raw_os_error(2), &path)),
        probe("spawn", &path)
    );
}
#[test]
fn sockets_keep_raw_address_in_connect_and_listen_errors() {
    let path = raw_path();
    assert_eq!(
        pi_js_compat::js_json!({"message":node_unix_connect_error(&io::Error::from_raw_os_error(2), &path)}),
        probe("connect", &path)
    );
    assert_eq!(
        pi_js_compat::js_json!({"message":node_unix_listen_error(&io::Error::from_raw_os_error(2), &path)}),
        probe("listen", &path)
    );
}
#[test]
fn nul_validation_inspects_raw_units_before_any_boundary_repair() {
    for path in [
        JsString::from("before-") + JsString::from_utf16(vec![0xd800, 0]) + "after",
        JsString::from("😀".repeat(80)) + "\0tail",
    ] {
        let expected = probe("nul", &path);
        let actual = node_path_argument_error(&path).unwrap();
        assert_eq!(actual.message, *expected["message"].as_string().unwrap());
        assert_eq!(actual.name, expected["name"].as_string().unwrap().to_utf8().unwrap());
        assert_eq!(actual.code, expected["code"].as_string().unwrap().to_utf8().unwrap());
    }
}
