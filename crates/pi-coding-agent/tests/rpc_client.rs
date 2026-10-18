use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_coding_agent::modes::rpc::{RpcClient, RpcClientOptions, RpcCommandBody};
use serde_json::{Value, json};
use tempfile::TempDir;

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

fn write_script(dir: &TempDir, contents: &str) -> PathBuf {
    let path = dir.path().join("child.mjs");
    std::fs::write(&path, contents).expect("child script must be written");
    path
}

fn options(script: PathBuf) -> RpcClientOptions {
    RpcClientOptions {
        cli_path: Some(script),
        ..RpcClientOptions::default()
    }
}

fn wait_for_file(path: &Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn sends_jsonl_commands_with_id_last_and_correlates_responses() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: command.type,
    success: true,
    data: {
      commands: [{
        name: "skill:review",
        description: "Review",
        source: "skill",
        sourceInfo: { path: "/tmp/review.md", source: "test", scope: "project", origin: "top-level" }
      }]
    }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let captured = std::sync::Arc::clone(&events);
    let _subscription = client.on_event(move |event| {
        captured.lock().expect("events").push(event.clone());
    });

    client.start().await.expect("client must start");
    let commands = client.get_commands().await.expect("commands must load");

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].name, "skill:review");
    assert_eq!(
        commands[0].source,
        pi_coding_agent::modes::rpc::RpcSlashCommandSource::Skill
    );

    {
        let events = events.lock().expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            json!({"type": "echo", "line": "{\"type\":\"get_commands\",\"id\":\"req_1\"}"})
        );
    }

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn rejects_in_flight_request_when_child_exits() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.stdin.once("data", () => {
  process.exit(43);
});
process.stdin.resume();
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let error = client.get_commands().await.expect_err("request must fail");
    assert_eq!(
        error.to_string(),
        "Agent process exited (code=43 signal=null). Stderr: "
    );
}

#[tokio::test]
async fn reports_startup_cancellation_when_stopped_during_startup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.on("SIGTERM", () => {});
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));

    let (start_result, stop_result) = tokio::join!(client.start(), client.stop());
    stop_result.expect("stop must succeed");
    let error = start_result.expect_err("start must be cancelled");
    assert_eq!(error.to_string(), "RPC client stopped during startup. Stderr: ");
}

#[tokio::test]
async fn rejects_pending_request_when_stopped() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.on("SIGTERM", () => {});
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let (send_result, stop_result) = tokio::join!(client.send(RpcCommandBody::GetCommands), client.stop());
    stop_result.expect("stop must succeed");
    let error = send_result.expect_err("request must fail");
    assert_eq!(error.to_string(), "RPC client stopped. Stderr: ");
}

#[tokio::test]
async fn rejects_commands_sent_after_stop_begins() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.on("SIGTERM", () => {});
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let (stop_result, send_result) = tokio::join!(client.stop(), client.send(RpcCommandBody::GetCommands));
    stop_result.expect("stop must succeed");
    let error = send_result.expect_err("request must fail");
    assert_eq!(error.to_string(), "RPC client is stopping. Stderr: ");
}

#[tokio::test]
async fn rejects_a_second_start() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.on("SIGTERM", () => {});
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let error = client.start().await.expect_err("second start must fail");
    assert_eq!(error.to_string(), "Client already started or stopping");

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn sends_sigkill_after_sigterm_is_ignored_and_waits_for_exit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pid_file = dir.path().join("pid");
    let term_file = dir.path().join("term");
    let script = write_script(
        &dir,
        &format!(
            r#"import {{ writeFileSync }} from "node:fs";
writeFileSync({pid_file:?}, String(process.pid));
process.on("SIGTERM", () => {{
  writeFileSync({term_file:?}, "term");
}});
process.stdin.resume();
setInterval(() => {{}}, 1000);
"#,
            pid_file = pid_file.display().to_string(),
            term_file = term_file.display().to_string(),
        ),
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");
    wait_for_file(&pid_file);

    client.stop().await.expect("stop must succeed");

    let pid = std::fs::read_to_string(&pid_file)
        .expect("pid file must exist")
        .trim()
        .to_owned();
    assert!(term_file.exists(), "child must have received SIGTERM first");
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "child must be gone after stop"
    );
}

#[tokio::test]
async fn collects_retry_events_through_the_final_agent_end() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  const emit = (event) => process.stdout.write(JSON.stringify(event) + "\n");
  emit({ type: "agent_end", messages: [], willRetry: true });
  emit({ type: "auto_retry_start", attempt: 1, maxAttempts: 3, delayMs: 100, errorMessage: "retry" });
  emit({ type: "agent_end", messages: [], willRetry: false });
  process.stdout.write(JSON.stringify({ id: command.id, type: "response", command: command.type, success: true }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let events = client
        .prompt_and_wait("Hello", None, EVENT_TIMEOUT)
        .await
        .expect("prompt must complete");
    assert_eq!(
        events,
        vec![
            json!({"type": "agent_end", "messages": [], "willRetry": true}),
            json!({
                "type": "auto_retry_start",
                "attempt": 1,
                "maxAttempts": 3,
                "delayMs": 100,
                "errorMessage": "retry"
            }),
            json!({"type": "agent_end", "messages": [], "willRetry": false}),
        ]
    );

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn waits_through_retrying_agent_ends() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "agent_end", messages: [], willRetry: true }) + "\n");
  process.stdout.write(JSON.stringify({ type: "agent_end", messages: [], willRetry: false }) + "\n");
  process.stdout.write(JSON.stringify({ id: command.id, type: "response", command: command.type, success: true }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let (send_result, idle_result) = tokio::join!(
        client.send(RpcCommandBody::Prompt {
            message: "Hello".to_owned(),
            images: None,
            streaming_behavior: None,
        }),
        client.wait_for_idle(EVENT_TIMEOUT)
    );
    send_result.expect("prompt must succeed");
    idle_result.expect("idle wait must resolve");

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn surfaces_command_failure_messages() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: command.type,
    success: false,
    error: "No API key found for fake-provider."
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let error = client.get_state().await.expect_err("state must fail");
    assert_eq!(error.to_string(), "No API key found for fake-provider.");

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn collects_child_stderr() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.stderr.write("oops\n");
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    client.start().await.expect("client must start");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !client.stderr().contains("oops") {
        assert!(std::time::Instant::now() < deadline, "stderr was not collected");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn delivers_unmatched_responses_as_events() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.stdout.write(JSON.stringify({ type: "response", id: "other", command: "x", success: true }) + "\n");
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(event["id"], json!("other"));

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn clone_sends_the_clone_command_and_returns_cancelled() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: "clone",
    success: true,
    data: { cancelled: false }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let result = client.clone_session().await.expect("clone must succeed");
    assert!(!result.cancelled);

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(
        event,
        json!({"type": "echo", "line": "{\"type\":\"clone\",\"id\":\"req_1\"}"})
    );

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn treats_responses_with_an_empty_id_as_events() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"process.stdout.write(JSON.stringify({ type: "response", id: "", command: "x", success: true }) + "\n");
process.stdin.resume();
setInterval(() => {}, 1000);
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(event["id"], json!(""));

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn compact_sends_custom_instructions_and_returns_the_result() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: "compact",
    success: true,
    data: {
      summary: "Condensed",
      firstKeptEntryId: "entry-2",
      tokensBefore: 1200,
      durationMs: 35,
      estimatedTokensAfter: 300
    }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let result = client
        .compact(Some("Keep decisions".to_owned()))
        .await
        .expect("compact must succeed");
    assert_eq!(result.summary, "Condensed");
    assert_eq!(result.first_kept_entry_id, "entry-2");
    assert_eq!(result.tokens_before, 1200.0);
    assert_eq!(result.duration_ms, Some(35.0));
    assert_eq!(result.estimated_tokens_after, Some(300.0));

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(
        event,
        json!({
            "type": "echo",
            "line": "{\"type\":\"compact\",\"customInstructions\":\"Keep decisions\",\"id\":\"req_1\"}"
        })
    );

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn compact_omits_absent_custom_instructions() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: "compact",
    success: true,
    data: { summary: "s", firstKeptEntryId: "e", tokensBefore: 1 }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let result = client.compact(None).await.expect("compact must succeed");
    assert_eq!(result.duration_ms, None);

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(
        event,
        json!({"type": "echo", "line": "{\"type\":\"compact\",\"id\":\"req_1\"}"})
    );

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn get_session_stats_returns_the_stats_payload() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: "get_session_stats",
    success: true,
    data: {
      sessionFile: "/tmp/session.jsonl",
      sessionId: "session-1",
      userMessages: 2,
      assistantMessages: 1,
      toolCalls: 3,
      toolResults: 3,
      totalMessages: 6,
      tokens: { input: 10, output: 20, cacheRead: 1, cacheWrite: 2, total: 33 },
      cost: 0.5,
      contextUsage: { tokens: 100, contextWindow: 1000, percent: 10 }
    }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let stats = client.get_session_stats().await.expect("stats must succeed");
    assert_eq!(stats.session_file.as_deref(), Some("/tmp/session.jsonl"));
    assert_eq!(stats.session_id, "session-1");
    assert_eq!(stats.user_messages, 2);
    assert_eq!(stats.assistant_messages, 1);
    assert_eq!(stats.tool_calls, 3);
    assert_eq!(stats.tool_results, 3);
    assert_eq!(stats.total_messages, 6);
    assert_eq!(stats.tokens.total, 33.0);
    assert_eq!(stats.cost, 0.5);
    assert_eq!(stats.context_usage.expect("context usage").percent, 10.0);

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(
        event,
        json!({"type": "echo", "line": "{\"type\":\"get_session_stats\",\"id\":\"req_1\"}"})
    );

    client.stop().await.expect("client must stop");
}

#[tokio::test]
async fn bash_sends_the_command_and_returns_the_result() {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = write_script(
        &dir,
        r#"import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const command = JSON.parse(line);
  process.stdout.write(JSON.stringify({ type: "echo", line }) + "\n");
  process.stdout.write(JSON.stringify({
    id: command.id,
    type: "response",
    command: "bash",
    success: true,
    data: { output: "hello\n", exitCode: 0, cancelled: false, truncated: false }
  }) + "\n");
});
"#,
    );
    let client = RpcClient::new(options(script));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(move |event| {
        let _ = sender.send(event.clone());
    });
    client.start().await.expect("client must start");

    let result = client.bash("echo hello").await.expect("bash must succeed");
    assert_eq!(result.output, "hello\n");
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.cancelled);
    assert!(!result.truncated);
    assert_eq!(result.full_output_path, None);

    let event = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("event must arrive")
        .expect("event channel must stay open");
    assert_eq!(
        event,
        json!({
            "type": "echo",
            "line": "{\"type\":\"bash\",\"command\":\"echo hello\",\"id\":\"req_1\"}"
        })
    );

    client.stop().await.expect("client must stop");
}
