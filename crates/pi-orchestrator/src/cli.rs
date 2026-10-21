//! Library CLI entry, preserving frozen text/commands without registering a new bin.
use crate::{
    config::{Config, VERSION},
    ipc::{
        client::send_ipc_request,
        protocol::{encode_message, parse_request_line},
    },
};
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
#[derive(Debug, PartialEq)]
pub enum CliAction {
    Output {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    Request(Value),
    Serve,
    RpcStream(String),
}

pub fn help() -> String {
    format!(
        "orchestrator v{VERSION}\n\nUsage:\n  orchestrator serve\n  orchestrator list\n  orchestrator spawn [--cwd <path>] [--label <label>]\n  orchestrator status <instance-id>\n  orchestrator stop <instance-id>\n  orchestrator rpc <instance-id> <json-command>\n  orchestrator rpc-stream <instance-id>\n  orchestrator --help\n  orchestrator --version\n\nRPC stream stdin expects JSONL RpcCommand or extension_ui_response messages.\n"
    )
}
fn output(stdout: String, stderr: String, exit_code: i32) -> CliAction {
    CliAction::Output {
        stdout,
        stderr,
        exit_code,
    }
}
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
pub fn parse_args(args: &[String], cwd: &str) -> Result<CliAction, String> {
    let command = args.first().map(String::as_str).unwrap_or("--help");
    match command {
        "--help" | "-h" => Ok(output(help(), String::new(), 0)),
        "--version" | "-v" => Ok(output(format!("{VERSION}\n"), String::new(), 0)),
        "serve" => Ok(CliAction::Serve),
        "list" => Ok(CliAction::Request(json!({"type":"list"}))),
        "spawn" => {
            let mut request = json!({"type":"spawn","cwd":flag_value(args,"--cwd").unwrap_or(cwd)});
            if let Some(label) = flag_value(args, "--label") {
                request["label"] = json!(label);
            }
            Ok(CliAction::Request(request))
        }
        "status" | "stop" | "rpc" | "rpc-stream" => parse_instance_command(command, args),
        _ => Ok(output(help(), format!("Unknown command: {command}\n"), 1)),
    }
}
fn parse_instance_command(command: &str, args: &[String]) -> Result<CliAction, String> {
    let id = args.get(1).filter(|v| !v.is_empty());
    let json = args.get(2).filter(|v| !v.is_empty());
    if id.is_none() || command == "rpc" && json.is_none() {
        let tail = if command == "rpc" { " <json-command>" } else { "" };
        return Ok(output(
            String::new(),
            format!("Usage: orchestrator {command} <instance-id>{tail}\n"),
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
    args: &[String],
    cwd: &str,
    config: Config,
    stdin: R,
    mut stdout: W,
    mut stderr: E,
) -> i32 {
    match execute_cli(args, cwd, config, stdin, &mut stdout, &mut stderr).await {
        Ok(code) => code,
        Err(error) => {
            if let Err(write_error) = stderr.write_all(format!("{error}\n").as_bytes()).await {
                eprintln!("CLI stderr: {write_error}");
            }
            1
        }
    }
}
async fn execute_cli<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    args: &[String],
    cwd: &str,
    config: Config,
    stdin: R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    match parse_args(args, cwd)? {
        CliAction::Output {
            stdout: text,
            stderr: error,
            exit_code,
        } => {
            stdout.write_all(text.as_bytes()).await.map_err(|e| e.to_string())?;
            stderr.write_all(error.as_bytes()).await.map_err(|e| e.to_string())?;
            Ok(exit_code)
        }
        CliAction::Request(request) => {
            let response = send_ipc_request(&config.socket_path(), &request).await?;
            let text = pi_js_compat::js_json::stringify_pretty(&response);
            stdout
                .write_all(format!("{text}\n").as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            Ok(0)
        }
        CliAction::RpcStream(id) => {
            rpc_stream(&config.socket_path(), &id, stdin, stdout, stderr).await?;
            Ok(0)
        }
        CliAction::Serve => {
            let runtime = crate::serve::RunningOrchestrator::start_default(config).await?;
            for line in &runtime.logs {
                stdout
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            runtime.run_until_signal().await?;
            Ok(0)
        }
    }
}
async fn rpc_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    path: &Path,
    id: &str,
    stdin: R,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), String> {
    let mut socket = tokio::net::UnixStream::connect(path).await.map_err(|e| e.to_string())?;
    socket
        .write_all(encode_message(&json!({"type":"rpc_stream","instanceId":id})).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stderr
        .write_all(
            format!("connected to rpc stream {id}; send JSONL RpcCommand or extension_ui_response on stdin\n")
                .as_bytes(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut stdin = BufReader::new(stdin).lines();
    let mut ended = false;
    let mut bytes = [0; 8192];
    loop {
        tokio::select! {
            result=socket.read(&mut bytes)=>{let count=result.map_err(|e|e.to_string())?;if count==0{return Ok(());}stdout.write_all(String::from_utf8_lossy(&bytes[..count]).as_bytes()).await.map_err(|e|e.to_string())?;},
            result=stdin.next_line(),if !ended=>match result.map_err(|e|e.to_string())?{
                Some(line) if !line.trim().is_empty()=>{let request=parse_request_line(line.trim())?;socket.write_all(encode_message(&request).as_bytes()).await.map_err(|e|e.to_string())?;},
                Some(_)=>{},None=>ended=true,
            }
        }
    }
}
