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
if "--debug" in args:
    print("fixture debug line", file=sys.stderr, flush=True)
    with open(args[args.index("--debug-file") + 1], "w", encoding="utf-8") as debug:
        debug.write("fixture CLI debug log\n")
assert args[:5] == ["--output-format", "stream-json", "--verbose", "--input-format", "stream-json"], args
assert os.environ.get("ENABLE_CLAUDEAI_MCP_SERVERS") == "0"
assert os.environ.get("DISABLE_AUTO_COMPACT") == "1"
assert os.environ.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC") == "1"
with open(".fixture-pid", "w", encoding="ascii") as pid_file:
    pid_file.write(str(os.getpid()))
if os.path.exists(".fixture-stall-initialize"):
    import time
    time.sleep(300)
init = read()
assert init["type"] == "control_request"
assert init["request"]["subtype"] == "initialize"
send({"type": "control_response", "response": {"subtype": "success", "request_id": init["request_id"], "response": {}}})
send({"type": "system", "subtype": "init", "session_id": "11111111-1111-4111-8111-111111111111"})
user = read()
assert user["type"] == "user", user
content = user["message"]["content"]
text = content if isinstance(content, str) else "".join(item.get("text", "") for item in content)
if "--max-turns" in args:
    assert "--resume" not in args and not any(arg.startswith("--resume=") for arg in args), args
    assert args[args.index("--tools") + 1] == "", args
    assert args[args.index("--setting-sources") + 1] == "", args
    assert "--no-session-persistence" in args and "--strict-mcp-config" in args
    assert "--permission-mode" not in args
    assert json.loads(args[args.index("--settings") + 1]) == {"autoMemoryEnabled": False}
    assert init["request"]["skills"] == []
    assert "appendSystemPrompt" not in init["request"]
    assert "systemPrompt" in init["request"]
    if text.startswith("part one"):
        assert init["request"]["systemPrompt"] == ["summary system \ud800"]
    if "summary-error" in text:
        send({"type": "result", "subtype": "success", "is_error": True, "result": "summary failed \udfff"})
    else:
        answer("Claude: " + text, "summary-msg")
elif text in ["assert-lifecycle-rebuild", "assert-session-cleared"]:
    resume = next((arg.split("=", 1)[1] for arg in args if arg.startswith("--resume=")), None)
    assert resume, args
    if text == "assert-session-cleared":
        assert resume != "11111111-1111-4111-8111-111111111111", args
    project = "".join(c if c.isascii() and c.isalnum() else "-" for c in os.getcwd())
    root = os.environ.get("CLAUDE_CONFIG_DIR", os.path.expanduser("~/.claude"))
    with open(os.path.join(root, "projects", project, resume + ".jsonl"), encoding="utf-8") as saved:
        records = [json.loads(line) for line in saved if line.strip()]
    assert records[0]["message"]["content"] == "hello", records
    assert records[1]["message"]["content"][0]["text"] == "Claude: hello", records
    answer("rebuilt", "rebuild-msg")
elif text == "account-quota":
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
elif text == "[Your context was compacted. What precedes this is a summary plus the most recent messages, ending with the tool result you were waiting for. Continue the task from there.]":
    resume = next((arg.split("=", 1)[1] for arg in args if arg.startswith("--resume=")), None)
    assert resume and resume != "11111111-1111-4111-8111-111111111111", args
    project = "".join(c if c.isascii() and c.isalnum() else "-" for c in os.getcwd())
    root = os.environ.get("CLAUDE_CONFIG_DIR", os.path.expanduser("~/.claude"))
    with open(os.path.join(root, "projects", project, resume + ".jsonl"), encoding="utf-8") as saved:
        records = [json.loads(line) for line in saved if line.strip()]
    assert records[0]["message"]["content"] == "compacted history", records
    answer("rewritten continuation", "rewrite-msg")
elif text in ["assert-ask-history", "assert-ask-existing"]:
    resume = next((arg.split("=", 1)[1] for arg in args if arg.startswith("--resume=")), None)
    assert resume, args
    assert "--no-session-persistence" not in args, args
    if text == "assert-ask-existing":
        assert resume == "existing-native", args
    else:
        project = "".join(c if c.isascii() and c.isalnum() else "-" for c in os.getcwd())
        root = os.environ["CLAUDE_CONFIG_DIR"]
        with open(os.path.join(root, "projects", project, resume + ".jsonl"), encoding="utf-8") as saved:
            records = [json.loads(line) for line in saved if line.strip()]
        assert records[0]["message"]["content"] == "prior question \udfff", records
        assert records[1]["message"]["content"][0]["text"] == "prior answer", records
    answer("shared history restored", "ask-history-msg")
elif text == "short-unowned":
    assert not any(arg.startswith("--resume=") for arg in args), args
    answer("short caller", "short-msg")
elif text == "assert-preserved-native":
    assert "--resume=11111111-1111-4111-8111-111111111111" in args, args
    answer("preserved", "preserved-msg")
elif text == "empty-tool-result":
    event({"type": "message_start", "message": {"id": "empty-tool-msg", "usage": {"input_tokens": 8}}})
    event({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "tool-read", "name": "mcp__custom-tools__read", "input": {"file_path": "empty"}}})
    event({"type": "content_block_stop", "index": 0})
    event({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 2}})
    event({"type": "message_stop"})
    result = control("empty-call", {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "read", "arguments": {}, "_meta": {"claudecode/toolUseId": "tool-read"}}})
    assert result["result"] == {"content": [{"type": "text", "text": ""}], "isError": True}, result
    send({"type": "result", "subtype": "success"})
elif text in ["empty-assistant-result", "missing-content-result", "empty-stream-result", "empty-is-error-result", "text-is-error-result", "empty-synthetic-error", "empty-stream-text"]:
    if text == "empty-assistant-result":
        send({"type": "assistant", "message": {"content": [{"type": "text", "text": ""}], "usage": {"input_tokens": 7, "output_tokens": 3}}})
    elif text == "missing-content-result":
        send({"type": "assistant", "message": {"usage": {"input_tokens": 99}}})
    elif text == "empty-stream-result":
        event({"type": "message_start", "message": {"id": "empty-msg", "usage": {"input_tokens": 7}}})
        event({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 3}})
        event({"type": "message_stop"})
    elif text == "empty-stream-text":
        answer("", "empty-text-msg")
        sys.exit(0)
    elif text == "empty-synthetic-error":
        send({"type": "assistant", "message": {"model": "<synthetic>", "content": [{"type": "text", "text": ""}], "usage": {"input_tokens": 7}}})
    result = {"type": "result", "subtype": "success"}
    if text == "empty-assistant-result":
        result["result"] = ""
    if text in ["empty-is-error-result", "text-is-error-result", "empty-synthetic-error"]:
        result["is_error"] = True
    if text == "text-is-error-result":
        result["result"] = "failed \udfff"
    send(result)
elif text == "terminating-tool":
    event({"type": "message_start", "message": {"id": "ending-msg"}})
    event({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "ending", "name": "mcp__custom-tools__end_turn", "input": {"reason": "done"}}})
    event({"type": "content_block_stop", "index": 0})
    event({"type": "message_delta", "delta": {"stop_reason": "tool_use"}})
    event({"type": "message_stop"})
    result = control("ending-call", {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "end_turn", "arguments": {}, "_meta": {"claudecode/toolUseId": "ending"}}})
    assert result["result"]["content"][0]["text"] == "complete \udfff", result
    send({"type": "control_request", "request_id": "post-batch", "request": {"subtype": "hook_callback", "callback_id": "hook_0", "input": {"hook_event_name": "PostToolBatch"}}})
    response = read()
    assert response["response"]["response"] == {"continue": False}, response
    with open(".fixture-end-turn-settled", "w", encoding="utf-8") as marker:
        marker.write("settled")
    send({"type": "result", "subtype": "success", "result": "", "session_id": "11111111-1111-4111-8111-111111111111"})
elif text in ["tool-round-trip", "steer-round-trip"]:
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
    call = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read", "arguments": {"path": "DO_NOT_EXECUTE_MCP_ARGUMENTS"}, "_meta": {"claudecode/toolUseId": "tool-read"}}}
    if text == "steer-round-trip":
        send({"type": "control_request", "request_id": "mcp-call", "request": {"subtype": "mcp_message", "server_name": "custom-tools", "message": call}})
        steer = read()
        assert steer["type"] == "user" and steer["priority"] == "next", steer
        assert steer["message"]["content"] == [{"type": "text", "text": "steer one\nsteer two"}], steer
        response = read()
        assert response["type"] == "control_response", response
        result = response["response"]["response"]["mcp_response"]
    else:
        result = control("mcp-call", call)
    with open(".fixture-tool-delivered", "w", encoding="ascii") as delivered:
        json.dump(result["result"]["content"][0]["text"], delivered, ensure_ascii=True)
    answer(result["result"]["content"][0]["text"], "final-msg")
elif text == "restream":
    event({"type": "message_start", "message": {"id": "msg_old"}})
    event({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
    event({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "abandoned content"}})
    answer("replacement content", "msg_new")
elif text == "rate-limit":
    for info in [
        {"status": "allowed_warning", "utilization": 0.81, "rateLimitType": "five_hour", "surpassedThreshold": 0.8},
        {"status": "allowed_warning", "utilization": 0.82, "rateLimitType": "five_hour", "surpassedThreshold": 0.8},
        {"status": "allowed_warning", "utilization": 0.86, "rateLimitType": "five_hour", "surpassedThreshold": 0.8},
        {"status": "allowed"},
        {"status": "allowed_warning", "utilization": 0.81, "rateLimitType": "five_hour", "surpassedThreshold": 0.8},
        {"status": "rejected", "rateLimitType": "five_hour"},
    ]:
        send({"type": "rate_limit_event", "rate_limit_info": info})
    send({"type": "result", "subtype": "success", "is_error": True, "result": "capacity \udfff"})
elif text == "error":
    send({"type": "result", "subtype": "success", "is_error": True, "result": "Quota \udfff"})
elif text == "auth-stderr-wait":
    print("OAuth invalid_grant status=401", file=sys.stderr, flush=True)
    while True:
        value = read()
        if value.get("request", {}).get("subtype") == "interrupt":
            break
elif text == "wait":
    while True:
        value = read()
        if value.get("request", {}).get("subtype") == "interrupt":
            break
else:
    answer("Claude: " + text, "answer-msg")
