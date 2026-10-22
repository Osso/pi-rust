//! One request per Unix socket, returning the first newline-delimited response.
use super::protocol::{encode_message, parse_response_line};
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub async fn send_ipc_request(path: &JsString, request: &Value) -> Result<Value, JsString> {
    let mut socket = UnixStream::connect(encode_node_utf8(path))
        .await
        .map_err(|error| pi_js_compat::node_fs::node_unix_connect_error(&error, path))?;
    socket
        .write_all(encode_node_utf8(&encode_message(request)).as_bytes())
        .await
        .map_err(|error| pi_js_compat::node_fs::node_pipe_write_error_record(&error).message)?;
    let mut buffer = String::new();
    let mut chunk = [0; 8192];
    loop {
        let count = socket
            .read(&mut chunk)
            .await
            .map_err(|error| pi_js_compat::node_fs::node_stream_error_record(&error, "read").message)?;
        if count == 0 {
            return Err(js_format!(
                "Orchestrator socket closed before a response was received: {}",
                path
            ));
        }
        buffer.push_str(&String::from_utf8_lossy(&chunk[..count]));
        if let Some(end) = buffer.find('\n') {
            let line = pi_js_compat::string::js_trim(&buffer[..end]);
            if !line.is_empty() {
                return parse_response_line(&JsString::from(line));
            }
        }
    }
}
