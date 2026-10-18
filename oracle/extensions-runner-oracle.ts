import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request = {
	id: RequestId;
	operation: "run";
	scenario: string;
};

type Logs = string[];

const sourceModulePath = process.argv[2];
if (!sourceModulePath) throw new Error("extensions runner oracle requires a source module path");
const packagesRoot = path.resolve(path.dirname(sourceModulePath), "../../../..");
const sourceUrl = (relative: string) => pathToFileURL(path.join(packagesRoot, relative)).href;

let modulesPromise: Promise<Record<string, any>> | undefined;
function loadModules(): Promise<Record<string, any>> {
	modulesPromise ??= (async () => {
		const [auth, loader, runner, keybindings, session, utils] = await Promise.all([
			import(sourceUrl("coding-agent/src/core/auth-storage.ts")),
			import(sourceUrl("coding-agent/src/core/extensions/loader.ts")),
			import(sourceUrl("coding-agent/src/core/extensions/runner.ts")),
			import(sourceUrl("coding-agent/src/core/keybindings.ts")),
			import(sourceUrl("coding-agent/src/core/session-manager.ts")),
			import(sourceUrl("coding-agent/test/model-runtime-test-utils.ts")),
		]);
		return { auth, loader, runner, keybindings, session, utils };
	})();
	return modulesPromise;
}

function normalizeString(value: string, tempDir: string): string {
	return value.split(tempDir).join("<tmp>");
}

function normalize(value: unknown, tempDir: string): unknown {
	if (typeof value === "string") return normalizeString(value, tempDir);
	if (Array.isArray(value)) return value.map((entry) => normalize(entry, tempDir));
	if (value && typeof value === "object") {
		const result: Record<string, unknown> = {};
		for (const [key, entry] of Object.entries(value)) {
			result[key] = normalize(entry, tempDir);
		}
		return result;
	}
	return value;
}

function jsonValue(value: unknown, tempDir: string): unknown {
	return value === undefined ? null : normalize(value, tempDir);
}

function logs(): Logs {
	const target = globalThis as unknown as { __extLogs?: Logs };
	target.__extLogs ??= [];
	return target.__extLogs;
}

const extensionActions: any = {
	sendMessage: () => {},
	sendUserMessage: () => {},
	appendEntry: () => {},
	requestResumeContinuation: () => {},
	setSessionName: () => {},
	getSessionName: () => undefined,
	setLabel: () => {},
	getActiveTools: () => [],
	getAllTools: () => [],
	setActiveTools: () => {},
	callTool: async () => ({ content: [], details: undefined }),
	callCommand: async () => undefined,
	refreshTools: () => {},
	getCommands: () => [],
	setModel: async () => false,
	getThinkingLevel: () => "off",
	setThinkingLevel: () => {},
};

const contextActions: any = {
	getModel: () => undefined,
	isIdle: () => true,
	hasActiveRetry: () => false,
	isProjectTrusted: () => true,
	getSignal: () => undefined,
	abort: () => {},
	hasPendingMessages: () => false,
	shutdown: () => {},
	restart: async () => {},
	getContextUsage: () => undefined,
	compact: () => {},
	getSystemPrompt: () => "",
	getScopedModels: () => [],
};

type Harness = {
	tempDir: string;
	extensionsDir: string;
	result: any;
	runner: any;
	errors: Array<{ extensionPath: string; event: string; error: string }>;
	providerCalls: unknown[];
	bind(providerActions?: any): void;
};

async function setup(files: Array<[string, string]>): Promise<Harness> {
	const modules = await loadModules();
	const { discoverAndLoadExtensions } = modules.loader;
	const { ExtensionRunner } = modules.runner;
	const { SessionManager } = modules.session;
	const { AuthStorage } = modules.auth;
	const { createInMemoryModelRegistry } = modules.utils;
	logs().length = 0;
	const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "pi-ext-oracle-"));
	const extensionsDir = path.join(tempDir, "extensions", "bundle");
	fs.mkdirSync(extensionsDir, { recursive: true });
	fs.writeFileSync(
		path.join(extensionsDir, "package.json"),
		JSON.stringify({ pi: { extensions: files.map(([name]) => name) } }),
	);
	for (const [name, code] of files) {
		fs.writeFileSync(path.join(extensionsDir, name), code);
	}
	const result = await discoverAndLoadExtensions([], tempDir, tempDir);
	const sessionManager = SessionManager.inMemory();
	const modelRegistry = await createInMemoryModelRegistry(AuthStorage.inMemory());
	const runner = new ExtensionRunner(result.extensions, result.runtime, tempDir, sessionManager, modelRegistry);
	const errors: Array<{ extensionPath: string; event: string; error: string }> = [];
	runner.onError((error) => {
		errors.push({
			extensionPath: normalizeString(error.extensionPath, tempDir),
			event: error.event,
			error: error.error,
		});
	});
	const providerCalls: unknown[] = [];
	const bind = (providerActions?: any) => {
		runner.bindCore(extensionActions, contextActions, providerActions);
	};
	return {
		tempDir,
		extensionsDir,
		result,
		runner,
		errors,
		providerCalls,
		bind,
	};
}

function customMessage(role: string, customType: string): unknown {
	return { role, customType, content: "c", display: true, timestamp: 1 };
}

function providerConfig() {
	return {
		baseUrl: "https://provider.test/v1",
		apiKey: "provider-test-key",
		api: "openai-completions",
		models: [
			{
				id: "instant-model",
				name: "Instant Model",
				reasoning: false,
				input: ["text"],
				cost: { input: 1, output: 2, cacheRead: 0.1, cacheWrite: 1.25 },
				contextWindow: 128000,
				maxTokens: 4096,
			},
		],
	};
}

function providerRecord(call: unknown): unknown {
	const record = call as { op: string; name?: string; config?: Record<string, unknown> };
	if (record.op !== "register") return record;
	const config = record.config ?? {};
	const models = Array.isArray(config.models) ? (config.models as Array<{ id: string }>) : undefined;
	return {
		op: record.op,
		name: record.name,
		baseUrl: config.baseUrl ?? null,
		api: config.api ?? null,
		apiKey: config.apiKey ?? null,
		authHeader: config.authHeader ?? null,
		modelIds: models ? models.map((model) => model.id) : null,
	};
}

async function dispatchScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const steps: unknown[] = [];

	await runner.emit({ type: "agent_start" });
	steps.push({ log: [...logs()] });

	const contextResult = await runner.emitContext([]);
	steps.push({ messages: contextResult });

	const toolResult = await runner.emitToolResult({
		type: "tool_result",
		toolName: "my_tool",
		toolCallId: "call-1",
		input: {},
		content: [{ type: "text", text: "base" }],
		details: { initial: true },
		isError: false,
	});
	steps.push({ toolResult });

	const beforeAgentStart = await runner.emitBeforeAgentStart("p", undefined, "base", { cwd: tempDir });
	steps.push({ beforeAgentStart });

	steps.push({ input: await runner.emitInput("x", undefined, "interactive") });

	const toolCallEvent = {
		type: "tool_call" as const,
		toolCallId: "call-2",
		toolName: "bash",
		bypassPermissions: false,
		input: { command: "pwd" },
	};
	steps.push({ toolCall: await runner.emitToolCall(toolCallEvent) });

	const headers = await runner.emitBeforeProviderHeaders({ "User-Agent": "x" });
	steps.push({ headers });

	steps.push({ resources: await runner.emitResourcesDiscover(tempDir, "startup") });

	steps.push({
		userBash: await runner.emitUserBash({ type: "user_bash", command: "ls", excludeFromContext: false, cwd: tempDir }),
	});

	steps.push({
		messageEnd: await runner.emitMessageEnd({
			type: "message_end",
			message: customMessage("custom", "m") as never,
		}),
	});

	await runner.emit({ type: "agent_end", messages: [] });
	steps.push({ log: [...logs()] });

	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function errorsScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const steps: unknown[] = [];

	steps.push({ context: await runner.emitContext([]) });

	try {
		await runner.emitToolCall({
			type: "tool_call",
			toolCallId: "call-1",
			toolName: "bash",
			bypassPermissions: false,
			input: { command: "pwd" },
		});
		steps.push({ toolCall: "no-error" });
	} catch (error) {
		steps.push({ toolCallError: error instanceof Error ? error.message : String(error) });
	}

	steps.push({
		messageEnd: await runner.emitMessageEnd({ type: "message_end", message: customMessage("user", "m") as never }),
	});

	const headers = await runner.emitBeforeProviderHeaders({ "User-Agent": "x" });
	steps.push({ headers });

	return { steps, errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function shortcutsScenario(harness: Harness) {
	const modules = await loadModules();
	const { KeybindingsManager } = modules.keybindings;
	const { runner, tempDir } = harness;
	const defaultKeybindings = new KeybindingsManager().getEffectiveConfig();
	const shortcuts = runner.getShortcuts(defaultKeybindings);
	const first = {
		keys: [...shortcuts.keys()],
		diagnostics: runner.getShortcutDiagnostics(),
	};

	const rebound = { ...defaultKeybindings, "app.interrupt": "ctrl+x" as never };
	const reboundShortcuts = runner.getShortcuts(rebound);
	const second = {
		keys: [...reboundShortcuts.keys()],
		diagnostics: runner.getShortcutDiagnostics(),
	};

	return {
		steps: [first, second].map((step) => normalize(step, tempDir)),
		errors: harness.errors.map((error) => normalize(error, tempDir)),
	};
}

async function commandsScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	const commands = runner.getRegisteredCommands();
	const steps = [
		{
			commands: commands.map((command) => ({
				name: command.name,
				invocationName: command.invocationName,
				description: command.description ?? null,
			})),
			diagnostics: runner.getCommandDiagnostics(),
		},
		{ byInvocation: runner.getCommand("shared-cmd:2")?.description ?? null },
		{ missing: runner.getCommand("not-exists") === undefined },
	];
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function flagsScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	const flags = runner.getFlags();
	const steps: unknown[] = [
		{
			flags: [...flags.entries()].map(([name, flag]) => ({
				name,
				description: flag.description ?? null,
				type: flag.type ?? null,
				default: flag.default ?? null,
			})),
		},
		{ values: [...harness.runner.getFlagValues().entries()] },
	];
	runner.setFlagValue("--test-flag", true);
	steps.push({ values: [...runner.getFlagValues().entries()] });
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function inputScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const images = [{ type: "image" as const, data: "orig", mimeType: "image/png" }];
	const steps = [
		{ continue: await runner.emitInput("x", undefined, "interactive") },
		{ transform: await runner.emitInput("hi", images, "interactive") },
		{ handled: await runner.emitInput("h", undefined, "interactive") },
	];
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function projectTrustScenario(harness: Harness) {
	const modules = await loadModules();
	const { emitProjectTrustEvent } = modules.runner;
	const { tempDir } = harness;
	const result = await emitProjectTrustEvent(
		harness.result,
		{ type: "project_trust", cwd: tempDir },
		{
			cwd: tempDir,
			mode: "tui",
			hasUI: false,
			ui: {
				select: async () => undefined,
				confirm: async () => false,
				input: async () => undefined,
				notify: () => {},
			},
		},
	);
	return {
		steps: normalize([{ result: result.result ?? null, errors: result.errors }], tempDir),
		errors: harness.errors.map((error) => normalize(error, tempDir)),
	};
}

async function providersScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	const providerActions: any = {
		registerProvider: (name, config) => {
			harness.providerCalls.push({ op: "register", name, config });
		},
		registerNativeProvider: (provider) => {
			harness.providerCalls.push({ op: "native", name: provider.id });
		},
		unregisterProvider: (name) => {
			harness.providerCalls.push({ op: "unregister", name });
		},
	};
	runner.bindCore(extensionActions, contextActions, providerActions);
	const steps: unknown[] = [
		{ calls: harness.providerCalls.map(providerRecord) },
		{ pending: runner["runtime"].pendingProviderRegistrations.length },
	];
	runner["runtime"].registerProvider("instant-provider", providerConfig(), "<inline:test>");
	steps.push({ calls: harness.providerCalls.map(providerRecord) });
	runner["runtime"].unregisterProvider("instant-provider", "<inline:test>");
	steps.push({ calls: harness.providerCalls.map(providerRecord) });
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function mailboxScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const message = {
		id: 1,
		recipient: { host: "h", sessionId: "r" },
		sender: { host: "h", sessionId: "s" },
		kind: "message",
		body: "hi",
		storeRef: { path: "/tmp/control.sqlite", rowId: 1 },
		status: "claimed",
		createdAt: "2026-01-01T00:00:00.000Z",
		updatedAt: "2026-01-01T00:00:00.000Z",
	};
	const steps: unknown[] = [];
	try {
		steps.push({ result: await runner.emitRuntimeMailbox({ type: "runtime_mailbox", message: message as never }) });
	} catch (error) {
		steps.push({ error: error instanceof Error ? error.message : String(error) });
	}
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function sessionEventsScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const preparation = {
		firstKeptEntryId: "e1",
		messagesToSummarize: [],
		turnPrefixMessages: [],
		isSplitTurn: false,
		tokensBefore: 10,
		fileOps: { read: new Set(), written: new Set(), edited: new Set() },
		settings: { enabled: true, reserveTokens: 1, keepRecentTokens: 2 },
	};
	const steps: unknown[] = [
		{
			switch: await runner.emit({
				type: "session_before_switch",
				reason: "new",
				targetSessionFile: "/tmp/s.jsonl",
			}),
		},
		{
			fork: await runner.emit({ type: "session_before_fork", entryId: "e", position: "at" }),
		},
		{
			beforeCompact: await runner.emit({
				type: "session_before_compact",
				preparation,
				branchEntries: [],
				reason: "manual",
				willRetry: false,
				signal: new AbortController().signal,
			}),
		},
		{
			compaction: await runner.emit({
				type: "compaction",
				preparation,
				branchEntries: [],
				reason: "manual",
				willRetry: false,
				signal: new AbortController().signal,
			}),
		},
		{
			source: await runner.emit({ type: "session_compaction_source", reason: "manual", willRetry: false }),
		},
		{ log: [...logs()] },
	];
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

async function mutationsScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const event = {
		type: "tool_call" as const,
		toolCallId: "call-mut",
		toolName: "bash",
		bypassPermissions: false,
		input: {} as Record<string, unknown>,
	};
	const toolCall = await runner.emitToolCall(event);
	const headers = await runner.emitBeforeProviderHeaders({ "User-Agent": "x" });
	return {
		steps: normalize(
			[
				{ toolCall, input: event.input },
				{ headers },
			],
			tempDir,
		),
		errors: harness.errors.map((error) => normalize(error, tempDir)),
	};
}

async function providerRequestScenario(harness: Harness) {
	const { runner, tempDir } = harness;
	harness.bind();
	const steps = [
		{ payload: await runner.emitBeforeProviderRequest({ base: true }) },
		{ log: [...logs()] },
	];
	return { steps: normalize(steps, tempDir), errors: harness.errors.map((error) => normalize(error, tempDir)) };
}

const SCENARIOS: Record<string, Array<[string, string]>> = {
	dispatch: [
		[
			"a.ts",
			`export default function(pi) {
	const log = (m) => globalThis.__extLogs.push(m);
	pi.on("agent_start", () => log("e1:agent_start"));
	pi.on("agent_end", (e) => log("e1:agent_end:" + e.messages.length));
	pi.on("context", (e) => { log("e1:context:" + e.messages.length); return { messages: [...e.messages, { role: "custom", customType: "e1", content: "x", display: true, timestamp: 1 }] }; });
	pi.on("tool_result", (e) => { log("e1:tool_result:" + e.content.length); return { content: [...e.content, { type: "text", text: "e1" }], details: { e1: true } }; });
	pi.on("before_agent_start", (e, ctx) => { log("e1:before_agent_start:" + ctx.getSystemPrompt()); return { systemPrompt: ctx.getSystemPrompt() + "|e1" }; });
	pi.on("input", (e) => ({ action: "transform", text: e.text + "[e1]" }));
	pi.on("tool_call", () => ({ reason: "e1" }));
	pi.on("before_provider_headers", (e) => { e.headers["X-E1"] = "1"; });
	pi.on("resources_discover", () => ({ skillPaths: ["s1"], promptPaths: ["p1"] }));
	pi.on("user_bash", (e) => { log("e1:user_bash:" + e.command); return { result: { output: "r1", exitCode: 0, cancelled: false, truncated: false } }; });
	pi.on("message_end", (e) => { log("e1:message_end:" + e.message.role); return { message: e.message }; });
}`,
		],
		[
			"b.ts",
			`export default function(pi) {
	const log = (m) => globalThis.__extLogs.push(m);
	pi.on("agent_start", () => log("e2:agent_start"));
	pi.on("context", (e) => { log("e2:context:" + e.messages.length); return { messages: [...e.messages, { role: "custom", customType: "e2", content: "y", display: true, timestamp: 2 }] }; });
	pi.on("tool_result", (e) => ({ content: [...e.content, { type: "text", text: "e2" }] }));
	pi.on("before_agent_start", (_e, ctx) => ({ systemPrompt: ctx.getSystemPrompt() + "|e2" }));
	pi.on("input", (e) => ({ action: "transform", text: e.text + "[e2]" }));
	pi.on("tool_call", () => undefined);
	pi.on("before_provider_headers", (e) => { e.headers["X-E2"] = "2"; });
	pi.on("resources_discover", () => ({ themePaths: ["t2"] }));
	pi.on("message_end", () => undefined);
}`,
		],
	],
	errors: [
		[
			"a.ts",
			`export default function(pi) {
	pi.on("context", () => { throw new Error("boom"); });
	pi.on("tool_call", () => { throw new Error("call boom"); });
	pi.on("message_end", () => ({ message: { role: "custom", customType: "x", content: "c", display: true, timestamp: 1 } }));
	pi.on("before_provider_headers", () => { throw new Error("header boom"); });
}`,
		],
	],
	shortcuts: [
		["a.ts", `export default function(pi) { pi.registerShortcut("ctrl+c", { handler: () => {} }); }`],
		["b.ts", `export default function(pi) { pi.registerShortcut("ctrl+p", { handler: () => {} }); }`],
		[
			"c.ts",
			`export default function(pi) { pi.registerShortcut("ctrl+v", { handler: () => {} }); }`,
		],
		["d.ts", `export default function(pi) { pi.registerShortcut("ctrl+shift+x", { handler: () => {} }); }`],
		["e.ts", `export default function(pi) { pi.registerShortcut("ctrl+shift+x", { handler: () => {} }); }`],
	],
	commands: [
		["a.ts", `export default function(pi) { pi.registerCommand("shared-cmd", { description: "First command", handler: async () => {} }); }`],
		["b.ts", `export default function(pi) { pi.registerCommand("shared-cmd", { description: "Second command", handler: async () => {} }); pi.registerCommand("solo", { description: "Solo", handler: async () => {} }); }`],
	],
	flags: [
		["a.ts", `export default function(pi) { pi.registerFlag("shared-flag", { description: "first", type: "boolean", default: true }); }`],
		["b.ts", `export default function(pi) { pi.registerFlag("shared-flag", { description: "second", type: "boolean", default: false }); pi.registerFlag("test-flag", { description: "Test flag", type: "boolean" }); }`],
	],
	input: [
		["a.ts", `export default function(pi) { pi.on("input", (e) => e.text === "h" ? ({ action: "handled" }) : ({ action: "transform", text: e.text + "[1]" })); }`],
		["b.ts", `export default function(pi) { pi.on("input", (e) => ({ action: "transform", text: e.text + "[2]" })); }`],
	],
	project_trust: [
		["a.ts", `export default function(pi) { pi.on("project_trust", () => ({ trusted: "undecided", remember: true })); }`],
		["b.ts", `export default function(pi) { pi.on("project_trust", () => ({ trusted: "no", remember: true })); }`],
	],
	providers: [
		["a.ts", `export default function(pi) {
	pi.registerProvider("queued", { baseUrl: "https://queued.test", apiKey: "k", api: "openai-completions", models: [{ id: "queued-model", name: "Queued", reasoning: false, input: ["text"], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 1000, maxTokens: 100 }] });
}`],
	],
	mailbox: [
		["a.ts", `export default function(pi) { pi.on("runtime_mailbox", () => ({ handled: true })); }`],
		["b.ts", `export default function(pi) { pi.on("runtime_mailbox", () => { throw new Error("mailbox boom"); }); }`],
	],
	session_events: [
		["a.ts", `export default function(pi) {
	const log = (m) => globalThis.__extLogs.push(m);
	pi.on("session_before_switch", () => { log("switch:a"); });
	pi.on("session_before_switch", () => ({ cancel: true }));
	pi.on("session_before_switch", () => { log("switch:c"); });
	pi.on("session_before_fork", () => ({ skipConversationRestore: true }));
	pi.on("session_before_compact", () => ({ cancel: true }));
	pi.on("session_before_compact", () => { log("compact:b"); });
	pi.on("compaction", () => ({ compaction: { summary: "s", firstKeptEntryId: "e1", tokensBefore: 3 } }));
	pi.on("compaction", () => { log("compaction:b"); });
	pi.on("session_compaction_source", () => ({ source: { type: "local", provider: "p", model: "m" } }));
	pi.on("session_compaction_source", () => ({ source: { type: "openai_remote", provider: "p2", model: "m2", endpoint: "https://e" } }));
}`],
	],
	mutations: [
		["a.ts", `export default function(pi) {
	pi.on("tool_call", (e) => { e.input.a = 1; });
	pi.on("tool_call", (e) => { e.input.b = e.input.a + 1; return { reason: "mut" }; });
	pi.on("before_provider_headers", (e) => { e.headers["X-1"] = "1"; });
	pi.on("before_provider_headers", (e) => { e.headers["X-2"] = e.headers["X-1"] + "2"; });
}`],
	],
	provider_request: [
		["a.ts", `export default function(pi) {
	pi.on("before_provider_request", (e) => ({ ...e.payload, a: 1 }));
	pi.on("before_provider_request", (e) => { e.payload.b = 2; });
	pi.on("before_provider_request", () => null);
}`],
	],
};

async function executeScenario(name: string): Promise<unknown> {
	const files = SCENARIOS[name];
	if (!files) throw new Error(`unknown scenario: ${name}`);
	const harness = await setup(files);
	try {
		switch (name) {
			case "dispatch":
				return await dispatchScenario(harness);
			case "errors":
				return await errorsScenario(harness);
			case "shortcuts":
				return await shortcutsScenario(harness);
			case "commands":
				return await commandsScenario(harness);
			case "flags":
				return await flagsScenario(harness);
			case "input":
				return await inputScenario(harness);
			case "project_trust":
				return await projectTrustScenario(harness);
			case "providers":
				return await providersScenario(harness);
			case "mailbox":
				return await mailboxScenario(harness);
			case "session_events":
				return await sessionEventsScenario(harness);
			case "mutations":
				return await mutationsScenario(harness);
			case "provider_request":
				return await providerRequestScenario(harness);
			default:
				throw new Error(`unhandled scenario: ${name}`);
		}
	} finally {
		fs.rmSync(harness.tempDir, { recursive: true, force: true });
	}
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

let requestQueue: Promise<void> = Promise.resolve();

function executeRequestLine(line: string): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		if (request.operation !== "run") throw new Error(`unknown operation: ${request.operation}`);
		requestQueue = requestQueue.then(async () => {
			try {
				const output = await executeScenario(request.scenario);
				process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
			} catch (error) {
				process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
			}
		});
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

let buffer = "";
process.stdin.on("data", (chunk: string | Buffer) => {
	buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
	while (true) {
		const lineFeed = buffer.indexOf("\n");
		if (lineFeed === -1) return;
		const line = buffer.slice(0, lineFeed);
		buffer = buffer.slice(lineFeed + 1);
		executeRequestLine(line);
	}
});
