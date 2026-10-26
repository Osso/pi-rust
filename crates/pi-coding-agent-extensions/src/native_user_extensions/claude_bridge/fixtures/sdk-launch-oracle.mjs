// Offline fixture generator. Usage: node sdk-launch-oracle.mjs <TS checkout> <output dir>
// The installed SDK and MCP packages are read-only; only the fake Claude process runs.
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

const [checkout, output] = process.argv.slice(2);
if (!checkout || !output) throw new Error("Expected TS checkout and output directory");
const require = createRequire(resolve(checkout, "package.json"));
const sdkPath = require.resolve("@anthropic-ai/claude-agent-sdk");
const { query } = await import(pathToFileURL(sdkPath).href);
const { McpServer } = await import(pathToFileURL(require.resolve("@modelcontextprotocol/sdk/server/mcp.js")).href);
const { ListToolsRequestSchema, CallToolRequestSchema } = await import(pathToFileURL(require.resolve("@modelcontextprotocol/sdk/types.js")).href);
const root = mkdtempSync(join(tmpdir(), "bridge-sdk-oracle-"));
const executable = join(root, "claude");
copyFileSync(fileURLToPath(new URL("claude.py", import.meta.url)), executable);
chmodSync(executable, 0o755);
mkdirSync(output, { recursive: true });
const excludes = ["**/CLAUDE.md", "**/AGENTS.md", "**/.claude/rules/**"];
const blocked = ["AskUserQuestion", "EnterPlanMode", "ExitPlanMode", "ToolSearch", "ScheduleWakeup"];
const readBlocked = ["Write", "Edit", "Bash", "NotebookEdit", "EnterWorktree", "ExitWorktree", "CronCreate", "CronDelete", "TeamCreate", "TeamDelete"];
const noneBlocked = ["Read", "Write", "Edit", "Glob", "Grep", "Bash", "Agent", "NotebookEdit", "EnterWorktree", "ExitWorktree", "CronCreate", "CronDelete", "TeamCreate", "TeamDelete", "WebFetch", "WebSearch"];

function createToolServer() {
    const instance = new McpServer({ name: "custom-tools", version: "1.0.0" }, { capabilities: { tools: {} } });
    instance.server.setRequestHandler(ListToolsRequestSchema, () => ({ tools: [{
        name: "edit", description: "Edit text", inputSchema: { type: "object", properties: { text: { type: "string" } } },
    }] }));
    instance.server.setRequestHandler(CallToolRequestSchema, async () => ({ content: [] }));
    return { "custom-tools": { type: "sdk", name: "custom-tools", instance } };
}

function queryOptions(mode, cwd) {
    const base = {
        cwd, pathToClaudeCodeExecutable: executable,
        env: { PATH: process.env.PATH, CLAUDE_CODE_ENTRYPOINT: "parent-entry", NODE_OPTIONS: "ignored", DEBUG: "ignored", ENABLE_CLAUDEAI_MCP_SERVERS: "0", DISABLE_AUTO_COMPACT: "1", CLAUDE_CONFIG_DIR: "/fixture/profile" },
    };
    if (mode.startsWith("summary")) return {
        ...base, tools: [], strictMcpConfig: true, settingSources: [], skills: [], persistSession: false,
        settings: { autoMemoryEnabled: false }, maxTurns: 1, model: "claude-opus-5-5[1m]",
        ...(mode === "summary" ? { systemPrompt: "summary \udfff" } : {}),
    };
    if (mode.startsWith("ask")) return {
        ...base, permissionMode: "bypassPermissions", skills: [], effort: "max", resume: "existing", persistSession: false,
        settings: { autoMemoryEnabled: false, claudeMdExcludes: excludes }, settingSources: ["user", "project"],
        systemPrompt: { type: "preset", preset: "claude_code", append: "append \ud800" },
        disallowedTools: [...blocked, ...(mode === "ask-read" ? readBlocked : mode === "ask-none" ? noneBlocked : [])],
        extraArgs: { "strict-mcp-config": null, model: "claude-opus-5-5[1m]", "thinking-display": "summarized" },
    };
    const configured = mode === "configured";
    return {
        ...base, tools: ["WebSearch"], permissionMode: "bypassPermissions", includePartialMessages: true,
        settings: { autoMemoryEnabled: configured, claudeMdExcludes: excludes, includeGitInstructions: false },
        systemPrompt: { type: "preset", preset: "claude_code", ...(!configured ? { append: "append \ud800" } : {}) },
        extraArgs: configured ? { model: "-custom-model" } : { model: "claude-opus-5-5[1m]", "strict-mcp-config": null, "thinking-display": "summarized" },
        hooks: { PostToolBatch: [{ hooks: [async () => ({})] }] },
        ...(!configured ? { effort: "xhigh", resume: "existing" } : {}),
        ...(mode === "tools" ? { mcpServers: createToolServer() } : {}),
        ...(mode === "debug" ? { debugFile: join(cwd, "debug.log") } : {}),
    };
}

async function captureLaunch(mode) {
    const cwd = join(root, mode);
    mkdirSync(cwd);
    writeFileSync(join(cwd, ".fixture-record-launch"), "");
    const stream = query({ prompt: "record", options: queryOptions(mode, cwd) });
    try {
        for await (const message of stream) {
            if (message.type === "result") break;
        }
    } finally {
        stream.close();
    }
    const record = JSON.parse(readFileSync(join(cwd, ".fixture-launch.json"), "utf8"));
    record.initialize.request_id = "<request-id>";
    record.cwd = "<cwd>";
    record.env = {}; // Rust asserts the complete inherited environment separately, without secret diffs.
    if (mode === "debug") record.argv[record.argv.indexOf("--debug-file") + 1] = "<debug-file>";
    writeFileSync(join(output, `sdk-launch-${mode}.json`), JSON.stringify(record, null, 2) + "\n");
}

try {
    for (const mode of ["provider", "tools", "ask", "summary", "summary-empty", "debug", "configured", "ask-read", "ask-none"]) {
        await captureLaunch(mode);
    }
    console.log(`sdk.mjs sha256: ${createHash("sha256").update(readFileSync(sdkPath)).digest("hex")}`);
} finally {
    rmSync(root, { recursive: true, force: true });
}
