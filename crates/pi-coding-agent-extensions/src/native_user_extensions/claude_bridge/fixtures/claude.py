#!/usr/bin/python3
"""Offline Claude stream-json peer. No account, sockets, SDK or network."""
import json
import os
import sys


def send(value):
    print(json.dumps(value, ensure_ascii=True, separators=(",", ":")), flush=True)


def read():
    line = sys.stdin.readline()
    if not line:
        raise RuntimeError("host closed input before the protocol completed")
    value = json.loads(line)
    log = os.environ.get("CLAUDE_BRIDGE_FIXTURE_LOG")
    if log:
        with open(log, "a", encoding="utf-8") as output:
            output.write(json.dumps(value, ensure_ascii=True) + "\n")
    return value


def control(request_id, message):
    send({"type": "control_request", "request_id": request_id,
          "request": {"subtype": "mcp_message", "server_name": "custom-tools", "message": message}})
    response = read()
    assert response["type"] == "control_response", response
    assert response["response"]["request_id"] == request_id, response
    return response["response"]["response"]["mcp_response"]


def event(value):
    send({"type": "stream_event", "event": value})


def answer(text, message_id):
    event({"type": "message_start", "message": {"id": message_id, "usage": {"input_tokens": 13, "cache_read_input_tokens": 40}}})
    event({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
    event({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}})
    event({"type": "content_block_stop", "index": 0})
    event({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 6}})
    event({"type": "message_stop"})
    send({"type": "assistant", "message": {"id": message_id, "model": "claude-opus-5-5", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}})
    send({"type": "result", "subtype": "success", "is_error": False, "result": text, "session_id": "11111111-1111-4111-8111-111111111111"})


args = sys.argv[1:]
if args[:1] == ["-p"]:
    assert args == ["-p", "Reply: ok", "--model", "haiku", "--max-turns", "1", "--tools", "", "--no-session-persistence", "--setting-sources", "project", "--strict-mcp-config"], args
    root = os.environ["CLAUDE_CONFIG_DIR"]
    with open(os.path.join(root, ".credentials.json"), "w", encoding="utf-8") as saved:
        json.dump({"claudeAiOauth": {"accessToken": "refreshed-for-usage", "expiresAt": 8640000000000}}, saved)
    sys.exit(1)  # Inference fails after persisting a successfully renewed grant.
assert args[:5] == ["--output-format", "stream-json", "--verbose", "--input-format", "stream-json"], args
assert os.environ.get("ENABLE_CLAUDEAI_MCP_SERVERS") == "0"
assert os.environ.get("DISABLE_AUTO_COMPACT") == "1"
assert os.environ.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC") == "1"
init = read()
assert init["type"] == "control_request"
assert init["request"]["subtype"] == "initialize"
send({"type": "control_response", "response": {"subtype": "success", "request_id": init["request_id"], "response": {}}})
send({"type": "system", "subtype": "init", "session_id": "11111111-1111-4111-8111-111111111111"})
user = read()
assert user["type"] == "user", user
content = user["message"]["content"]
text = content if isinstance(content, str) else "".join(item.get("text", "") for item in content)
if text == "account-quota":
    root = os.environ["CLAUDE_CONFIG_DIR"]
    name = os.path.basename(root)
    if name == "a":
        import time
        send({"type": "rate_limit_event", "rate_limit_info": {"status": "rejected", "rateLimitType": "five_hour", "resetsAt": time.time() + 3600}})
        send({"type": "result", "subtype": "success", "is_error": True, "result": "Rate limited"})
    else:
        assert name == "b", root
        with open(os.path.join(root, ".credentials.json"), "w", encoding="utf-8") as saved:
            json.dump({"claudeAiOauth": {"accessToken": "refreshed"}}, saved)
        answer("account b", "account-msg")
elif text == "assert-history":
    resume = next((arg.split("=", 1)[1] for arg in args if arg.startswith("--resume=")), None)
    assert resume, "history must be imported and resumed"
    project = "".join(c if c.isascii() and c.isalnum() else "-" for c in os.getcwd())
    root = os.environ.get("CLAUDE_CONFIG_DIR", os.path.expanduser("~/.claude"))
    with open(os.path.join(root, "projects", project, resume + ".jsonl"), encoding="utf-8") as saved:
        records = [json.loads(line) for line in saved if line.strip()]
    assert records[0]["message"]["content"] == "earlier \udfff", records
    assert records[1]["parentUuid"] == records[0]["uuid"], records
    assert records[1]["message"]["content"][0]["text"] == "Claude: earlier \udfff", records
    assert all(record["sessionId"] == resume for record in records), records
    answer("history restored", "history-msg")
elif text == "tool-round-trip":
    manifest = control("mcp-init", {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "fake-claude", "version": "1"}}})
    assert manifest["result"]["capabilities"]["tools"] == {}, manifest
    tools = control("mcp-list", {"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    tool = next(item for item in tools["result"]["tools"] if item["name"] == "read")
    assert tool["inputSchema"]["type"] == "object", tool
    event({"type": "message_start", "message": {"id": "tool-msg"}})
    event({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "tool-read", "name": "mcp__custom-tools__read", "input": {}}})
    event({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": '{"path":"fixture.txt"}'}})
    event({"type": "content_block_stop", "index": 0})
    event({"type": "message_delta", "delta": {"stop_reason": "tool_use"}})
    event({"type": "message_stop"})
    result = control("mcp-call", {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read", "arguments": {"path": "DO_NOT_EXECUTE_MCP_ARGUMENTS"}, "_meta": {"claudecode/toolUseId": "tool-read"}}})
    answer(result["result"]["content"][0]["text"], "final-msg")
elif text == "error":
    send({"type": "result", "subtype": "success", "is_error": True, "result": "Quota \udfff"})
elif text == "wait":
    while True:
        value = read()
        if value.get("request", {}).get("subtype") == "interrupt":
            break
else:
    answer("Claude: " + text, "answer-msg")
