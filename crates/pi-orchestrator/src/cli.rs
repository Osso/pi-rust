//! Library CLI entry, preserving frozen text/commands without registering a new bin.
use crate::{
    config::{Config, VERSION},
    ipc::{
        client::send_ipc_request,
        protocol::{encode_message, parse_request_line},
    },
};
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
#[derive(Debug, PartialEq)]
pub enum CliAction {
    Output {
        stdout: JsString,
        stderr: JsString,
        exit_code: i32,
    },
    Request(Value),
    Serve,
    RpcStream(JsString),
}

pub fn help() -> JsString {
    js_format!(
        "orchestrator v{}\n\nUsage:\n  orchestrator serve\n  orchestrator list\n  orchestrator spawn [--cwd <path>] [--label <label>]\n  orchestrator status <instance-id>\n  orchestrator stop <instance-id>\n  orchestrator rpc <instance-id> <json-command>\n  orchestrator rpc-stream <instance-id>\n  orchestrator --help\n  orchestrator --version\n\nRPC stream stdin expects JSONL RpcCommand or extension_ui_response messages.\n",
        VERSION
    )
}
fn output(stdout: JsString, stderr: JsString, exit_code: i32) -> CliAction {
    CliAction::Output {
        stdout,
        stderr,
        exit_code,
    }
}
fn flag_value<'a>(args: &'a [JsString], flag: &str) -> Option<&'a JsString> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1))
}
pub fn parse_args(args: &[JsString], cwd: &JsString) -> Result<CliAction, JsString> {
    let help_command = JsString::from("--help");
    let command = args.first().unwrap_or(&help_command);
    match command {
        command if command == "--help" || command == "-h" => Ok(output(help(), JsString::default(), 0)),
        command if command == "--version" || command == "-v" => {
            Ok(output(js_format!("{}\n", VERSION), JsString::default(), 0))
        }
        command if command == "serve" => Ok(CliAction::Serve),
        command if command == "list" => Ok(CliAction::Request(json!({"type":"list"}))),
        command if command == "spawn" => {
            let mut request = json!({"type":"spawn","cwd":flag_value(args,"--cwd").unwrap_or(cwd)});
            if let Some(label) = flag_value(args, "--label") {
                request["label"] = json!(label);
            }
            Ok(CliAction::Request(request))
        }
        command if command == "status" || command == "stop" || command == "rpc" || command == "rpc-stream" => {
            parse_instance_command(command, args)
        }
        _ => Ok(output(help(), js_format!("Unknown command: {}\n", command), 1)),
    }
}
fn parse_instance_command(command: &JsString, args: &[JsString]) -> Result<CliAction, JsString> {
    let id = args.get(1).filter(|v| !v.is_empty());
    let json = args.get(2).filter(|v| !v.is_empty());
    if id.is_none() || command == "rpc" && json.is_none() {
        let tail = if command == "rpc" { " <json-command>" } else { "" };
        return Ok(output(
            JsString::default(),
            js_format!("Usage: orchestrator {} <instance-id>{}\n", command, tail),
            1,
        ));
    }
    let id = id.unwrap();
    if command == "rpc-stream" {
        return Ok(CliAction::RpcStream(id.clone()));
    }
    let mut request = json!({"type":command,"instanceId":id});
    if command == "rpc" {
        request["command"] = parse_request_line(json.unwrap())?;
    }
    Ok(CliAction::Request(request))
}

/// Returns the frozen exit code; embedding executable owns process termination.
pub async fn run_cli<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    args: &[JsString],
    cwd: &JsString,
    config: Config,
    stdin: R,
    mut stdout: W,
    mut stderr: E,
) -> i32 {
    // Frozen cli.ts statically imports serve.ts -> radius.ts; its auth storage is
    // initialized before main dispatch, even for --help/--version. No catch exists.
    let radius_options =
        crate::radius::RadiusOptions::from_environment().unwrap_or_else(|error| error.raise_uncaught());
    match execute_cli(args, cwd, config, radius_options, stdin, &mut stdout, &mut stderr).await {
        Ok(code) => code,
        Err(error) => {
            if let Err(write_error) = stderr
                .write_all(encode_node_utf8(&js_format!("{}\n", error)).as_bytes())
                .await
            {
                eprintln!("CLI stderr: {write_error}");
            }
            1
        }
    }
}
async fn execute_cli<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    args: &[JsString],
    cwd: &JsString,
    config: Config,
    radius_options: crate::radius::RadiusOptions,
    stdin: R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, JsString> {
    match parse_args(args, cwd)? {
        CliAction::Output {
            stdout: text,
            stderr: error,
            exit_code,
        } => {
            stdout
                .write_all(encode_node_utf8(&text).as_bytes())
                .await
                .map_err(|e| JsString::from(e.to_string()))?;
            stderr
                .write_all(encode_node_utf8(&error).as_bytes())
                .await
                .map_err(|e| JsString::from(e.to_string()))?;
            Ok(exit_code)
        }
        CliAction::Request(request) => {
            let response = send_ipc_request(&config.socket_path(), &request).await?;
            let text = pi_js_compat::js_json::stringify_lossless_pretty(&response);
            stdout
                .write_all(encode_node_utf8(&js_format!("{}\n", text)).as_bytes())
                .await
                .map_err(|e| JsString::from(e.to_string()))?;
            Ok(0)
        }
        CliAction::RpcStream(id) => {
            rpc_stream(&config.socket_path(), &id, stdin, stdout, stderr).await?;
            Ok(0)
        }
        CliAction::Serve => {
            run_serve(config, radius_options, stdout).await?;
            Ok(0)
        }
    }
}
async fn rpc_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    path: &JsString,
    id: &JsString,
    stdin: R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), JsString> {
    let mut socket = connect_rpc_stream(path, id, stderr).await?;
    let mut stdin = BufReader::new(stdin);
    let mut input_line = Vec::new();
    let mut ended = false;
    let mut bytes = [0; 8192];
    loop {
        tokio::select! {
            result = socket.read(&mut bytes) => {
                let count = result.map_err(|error| pi_js_compat::node_fs::node_stream_error_record(&error, "read").message)?;
                if count == 0 { return Ok(()); }
                let text = JsString::from(String::from_utf8_lossy(&bytes[..count]).as_ref());
                stdout.write_all(encode_node_utf8(&text).as_bytes()).await.map_err(|e| JsString::from(e.to_string()))?;
            },
            result = stdin.read_until(b'\n', &mut input_line), if !ended => {
                result.map_err(|error| pi_js_compat::node_fs::node_stream_error_record(&error, "read").message)?;
                ended = forward_complete_line(&mut socket, &input_line).await?;
                input_line.clear();
            }
        }
    }
}

async fn connect_rpc_stream<E: AsyncWrite + Unpin>(
    path: &JsString,
    id: &JsString,
    stderr: &mut E,
) -> Result<tokio::net::UnixStream, JsString> {
    let mut socket = tokio::net::UnixStream::connect(encode_node_utf8(path))
        .await
        .map_err(|error| pi_js_compat::node_fs::node_unix_connect_error(&error, path))?;
    let request = json!({"type":"rpc_stream","instanceId":id});
    socket
        .write_all(encode_node_utf8(&encode_message(&request)).as_bytes())
        .await
        .map_err(|error| pi_js_compat::node_fs::node_pipe_write_error_record(&error).message)?;
    let status = js_format!(
        "connected to rpc stream {}; send JSONL RpcCommand or extension_ui_response on stdin\n",
        id
    );
    stderr
        .write_all(encode_node_utf8(&status).as_bytes())
        .await
        .map_err(|e| JsString::from(e.to_string()))?;
    Ok(socket)
}

async fn forward_complete_line(socket: &mut tokio::net::UnixStream, bytes: &[u8]) -> Result<bool, JsString> {
    if bytes.last() != Some(&b'\n') {
        return Ok(true);
    }
    let text = String::from_utf8_lossy(bytes);
    let line = pi_js_compat::string::js_trim(&text);
    if !line.is_empty() {
        let request = parse_request_line(&JsString::from(line))?;
        socket
            .write_all(encode_node_utf8(&encode_message(&request)).as_bytes())
            .await
            .map_err(|error| pi_js_compat::node_fs::node_pipe_write_error_record(&error).message)?;
    }
    Ok(false)
}

async fn run_serve<W: AsyncWrite + Unpin>(
    config: Config,
    radius_options: crate::radius::RadiusOptions,
    stdout: &mut W,
) -> Result<(), JsString> {
    let runtime = crate::serve::RunningOrchestrator::start_with_radius_options(config, radius_options).await?;
    for line in &runtime.logs {
        stdout
            .write_all(encode_node_utf8(&js_format!("{}\n", line)).as_bytes())
            .await
            .map_err(|e| JsString::from(e.to_string()))?;
    }
    runtime.run_until_signal().await
}
