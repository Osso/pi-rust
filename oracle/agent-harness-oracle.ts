import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | null;

interface Request {
	id: RequestId;
	operation: string;
}

interface Response {
	id: RequestId;
	ok: boolean;
	output?: unknown;
	error?: { name: string; message: string; operation?: string };
}

// Source modules are loaded from the pinned worktree at runtime; keep the oracle
// independent of this repository's module graph.
type AgentHarnessApi = any;
type TelemetryApi = any;
type SessionApi = any;
type ModelsApi = any;
type CompatApi = any;
type TelemetryPackageApi = any;

interface Api {
	harness: AgentHarnessApi;
	telemetry: TelemetryApi;
	session: SessionApi;
	models: ModelsApi;
	compat: CompatApi;
	telemetryPackage: TelemetryPackageApi;
}

function normalizeError(error: unknown): NonNullable<Response["error"]> {
	if (error instanceof Error) {
		const operation = (error as Error & { operation?: string }).operation;
		return {
			name: error.name,
			message: error.message,
			...(operation === undefined ? {} : { operation }),
		};
	}
	return { name: "Unknown", message: String(error) };
}

function createSession(api: Api, id = "session"): InstanceType<SessionApi["Session"]> {
	return new api.session.Session(new api.session.InMemorySessionStorage({ id, createdAt: 1 }));
}

function createOptions(api: Api, session: InstanceType<SessionApi["Session"]>): AgentHarnessApi["AgentHarnessOptions"] {
	return {
		session,
		models: api.models.createModels(),
		model: api.compat.getModel("google", "gemini-2.5-flash") as never,
	};
}

async function createHarness(api: Api): Promise<AgentHarnessApi["AgentHarness"]> {
	const created = await api.harness.AgentHarness.create(createOptions(api, createSession(api)));
	return created.harness;
}

function resourceNames(resources: { skills?: { name: string }[]; promptTemplates?: { name: string }[] }): unknown {
	return {
		skills: resources.skills ? resources.skills.map((skill) => skill.name) : null,
		promptTemplates: resources.promptTemplates
			? resources.promptTemplates.map((template) => template.name)
			: null,
	};
}

function modelIdentity(model: { provider: string; id: string }): unknown {
	return { provider: model.provider, id: model.id };
}

async function harnessDefaults(api: Api): Promise<unknown> {
	const harness = await createHarness(api);
	const resources = await harness.getResources();
	const retry = await harness.getRetryPolicy();
	const compaction = await harness.getCompactionSettings();
	const streamOptions = await harness.getStreamOptions();
	return {
		name: harness.name,
		leafId: await harness.getLeafId(),
		model: modelIdentity(await harness.getModel()),
		thinkingLevel: await harness.getThinkingLevel(),
		activeTools: await harness.getActiveTools(),
		tools: (await harness.getTools()).map((tool) => tool.name),
		resources: resourceNames(resources),
		streamOptions: { maxTokens: streamOptions.maxTokens ?? null },
		retry: { enabled: retry.enabled, maxRetries: retry.maxRetries, baseDelayMs: retry.baseDelayMs },
		compaction: {
			enabled: compaction.enabled,
			reserveTokens: compaction.reserveTokens,
			keepRecentTokens: compaction.keepRecentTokens,
		},
		steeringMode: await harness.getSteeringMode(),
		followUpMode: await harness.getFollowUpMode(),
	};
}

async function harnessConfigRoundtrip(api: Api): Promise<unknown> {
	const harness = await createHarness(api);
	await harness.setModel(api.compat.getModel("anthropic", "claude-sonnet-4-5") as never);
	await harness.setThinkingLevel("high");

	const activeTools = ["one"];
	await harness.setActiveTools(activeTools);
	activeTools.push("mutated");
	const readActiveTools = await harness.getActiveTools();
	readActiveTools.push("mutated");

	const tools = [{ name: "tool", label: "Tool" } as never];
	await harness.setTools(tools);
	tools.push({ name: "mutated", label: "Mutated" } as never);
	const readTools = await harness.getTools();
	readTools.push({ name: "mutated", label: "Mutated" } as never);

	const resources = {
		skills: [{ name: "skill", description: "desc", content: "body", filePath: "/tmp/SKILL.md" }],
		promptTemplates: [{ name: "template", content: "body" }],
	};
	await harness.setResources(resources);
	resources.skills.push({ name: "mutated", description: "desc", content: "body", filePath: "/tmp/OTHER.md" });
	const readResources = await harness.getResources();
	readResources.skills?.push({ name: "mutated", description: "desc", content: "body", filePath: "/tmp/OTHER.md" });

	const streamOptions = { maxTokens: 10 };
	await harness.setStreamOptions(streamOptions);
	streamOptions.maxTokens = 20;
	const readStreamOptions = await harness.getStreamOptions();
	readStreamOptions.maxTokens = 30;

	const retryPolicy = { enabled: true, maxRetries: 2, baseDelayMs: 10 };
	await harness.setRetryPolicy(retryPolicy);
	retryPolicy.maxRetries = 99;

	const compactionSettings = { enabled: false, reserveTokens: 1, keepRecentTokens: 2 };
	await harness.setCompactionSettings(compactionSettings);
	compactionSettings.reserveTokens = 99;

	await harness.setSteeringMode("all");
	await harness.setFollowUpMode("all");

	const finalResources = await harness.getResources();
	const finalRetry = await harness.getRetryPolicy();
	const finalCompaction = await harness.getCompactionSettings();
	const finalStreamOptions = await harness.getStreamOptions();
	return {
		model: modelIdentity(await harness.getModel()),
		thinkingLevel: await harness.getThinkingLevel(),
		activeTools: await harness.getActiveTools(),
		tools: (await harness.getTools()).map((tool) => tool.name),
		resources: resourceNames(finalResources),
		streamOptions: { maxTokens: finalStreamOptions.maxTokens ?? null },
		retry: {
			enabled: finalRetry.enabled,
			maxRetries: finalRetry.maxRetries,
			baseDelayMs: finalRetry.baseDelayMs,
		},
		compaction: {
			enabled: finalCompaction.enabled,
			reserveTokens: finalCompaction.reserveTokens,
			keepRecentTokens: finalCompaction.keepRecentTokens,
		},
		steeringMode: await harness.getSteeringMode(),
		followUpMode: await harness.getFollowUpMode(),
	};
}

const userMessage = { role: "user", content: [{ type: "text", text: "hello" }], timestamp: 1 };
const usage = {
	input: 1,
	output: 2,
	cacheRead: 0,
	cacheWrite: 0,
	totalTokens: 3,
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function unfinishedOperations(harness: AgentHarnessApi["AgentHarness"], callback: () => void) {
	return [
		["prompt", () => harness.prompt("hello")],
		["skill", () => harness.skill("skill")],
		["promptFromTemplate", () => harness.promptFromTemplate("template")],
		["compact", () => harness.compact()],
		["navigateTree", () => harness.navigateTree(null)],
		["resume", () => harness.resume()],
		["abort", () => harness.abort()],
		["steer", () => harness.steer(userMessage as never)],
		["followUp", () => harness.followUp(userMessage as never)],
		["nextRun", () => harness.nextRun(userMessage as never)],
		["cancelQueued", () => harness.cancelQueued("queued")],
		["recordUsage", () => harness.recordUsage(usage as never)],
		["waitForIdle", () => harness.waitForIdle()],
		["runWhenIdle", () => harness.runWhenIdle(callback)],
		["peekAction", () => harness.peekAction()],
		["executeAction", () => harness.executeAction()],
		["runToCompletion", () => harness.runToCompletion()],
		["watch", () => harness.watch()],
		["lane", () => harness.lane("main")],
		["createLane", () => harness.createLane("thread", null)],
		["lanes", () => harness.lanes()],
		["watchSession", () => harness.watchSession()],
	] as const;
}

async function harnessUnfinished(api: Api): Promise<unknown> {
	const harness = await createHarness(api);
	let callbackCalled = false;
	const rejections: unknown[] = [];
	for (const [operation, invoke] of unfinishedOperations(harness, () => {
		callbackCalled = true;
	})) {
		try {
			await invoke();
			rejections.push({ operation, resolved: true });
		} catch (error) {
			const normalized = normalizeError(error);
			rejections.push({
				operation,
				name: normalized.name,
				operationField: normalized.operation ?? null,
			});
		}
	}
	await harness.close();
	const closed: unknown[] = [];
	for (const [operation, invoke] of unfinishedOperations(harness, () => {
		callbackCalled = true;
	})) {
		try {
			await invoke();
			closed.push({ operation, resolved: true });
		} catch (error) {
			closed.push({ operation, name: normalizeError(error).name });
		}
	}

	let hooksError: unknown;
	try {
		harness.hooks.on("before_run", () => {});
	} catch (error) {
		hooksError = normalizeError(error);
	}
	let eventsError: unknown;
	try {
		harness.events.on("event", () => {});
	} catch (error) {
		eventsError = normalizeError(error);
	}
	return {
		rejections,
		closed,
		callbackCalled,
		hooksError: {
			name: (hooksError as { name: string }).name,
			operation: (hooksError as { operation?: string }).operation ?? null,
		},
		eventsError: {
			name: (eventsError as { name: string }).name,
			operation: (eventsError as { operation?: string }).operation ?? null,
		},
	};
}

async function harnessCreateEmpty(api: Api): Promise<unknown> {
	const session = createSession(api);
	const created = await api.harness.AgentHarness.create(createOptions(api, session));
	const leafIdBefore = await created.harness.getLeafId();
	const entryId = await session.appendMessage({
		role: "user",
		content: [{ type: "text", text: "hello" }],
		timestamp: 1,
	} as never);
	const leafIdAfter = await created.harness.getLeafId();
	await created.harness.close();
	return {
		suspended: created.suspended.length,
		name: created.harness.name,
		leafIdBefore,
		leafMatchesAppended: leafIdAfter === entryId,
	};
}

async function harnessCreateRestore(api: Api): Promise<unknown> {
	const session = createSession(api, "recorded");
	await session.appendRecord({
		type: "operation_started",
		id: "run",
		lane: "main",
		sourceLeafId: null,
		intent: { kind: "run", originalPrompt: [], initialMessages: [] },
	} as never);
	try {
		await api.harness.AgentHarness.create(createOptions(api, session));
		return { resolved: true };
	} catch (error) {
		const normalized = normalizeError(error);
		return { name: normalized.name, operation: normalized.operation ?? null };
	}
}

async function telemetrySchemas(api: Api): Promise<unknown> {
	return {
		ai: api.telemetry.AI_TELEMETRY_SCHEMA,
		harness: api.telemetry.HARNESS_TELEMETRY_SCHEMA,
		agent: api.telemetry.AGENT_TELEMETRY_SCHEMAS.map((schema) => Object.keys(schema.spans).sort()),
	};
}

async function telemetrySpans(api: Api): Promise<unknown> {
	const context = new api.telemetryPackage.InMemoryTelemetryContext();
	await api.telemetry.startAiSpan(
		context,
		"pi.ai.request",
		{
			"pi.ai.operation": "stream",
			"pi.ai.provider": "provider",
			"pi.ai.model": "model",
			"pi.ai.api": "api",
			"pi.ai.streaming": true,
		},
		async (span) => {
			span.setAttributes({ "pi.ai.response.stop_reason": "tool_use" });
		},
	);
	await api.telemetry.startHarnessSpan(
		context,
		"pi.harness.run",
		{
			"pi.session.id": "session",
			"pi.lane.name": "main",
			"pi.operation.id": "operation",
			"pi.operation.kind": "run",
			"pi.operation.recovery": false,
		},
		async (span) => {
			span.setAttributes({ "pi.operation.outcome": "completed" });
		},
	);
	return context.getSpans();
}

async function execute(request: Request, api: Api): Promise<Response> {
	switch (request.operation) {
		case "harness_defaults":
			return { id: request.id, ok: true, output: await harnessDefaults(api) };
		case "harness_config_roundtrip":
			return { id: request.id, ok: true, output: await harnessConfigRoundtrip(api) };
		case "harness_unfinished":
			return { id: request.id, ok: true, output: await harnessUnfinished(api) };
		case "harness_create_empty":
			return { id: request.id, ok: true, output: await harnessCreateEmpty(api) };
		case "harness_create_restore":
			return { id: request.id, ok: true, output: await harnessCreateRestore(api) };
		case "telemetry_schemas":
			return { id: request.id, ok: true, output: await telemetrySchemas(api) };
		case "telemetry_spans":
			return { id: request.id, ok: true, output: await telemetrySpans(api) };
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function executeRequestLine(line: string, api: Api): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: Api): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, api));
		}
	});
}

function readSourceModules(): string[] {
	const modules = process.argv.slice(2);
	if (modules.length !== 6) throw new Error("agent harness oracle requires six source module paths");
	return modules;
}

async function main(): Promise<void> {
	const [harnessPath, telemetryPath, sessionPath, modelsPath, compatPath, telemetryPackagePath] = readSourceModules();
	if (!harnessPath || !telemetryPath || !sessionPath || !modelsPath || !compatPath || !telemetryPackagePath) {
		throw new Error("agent harness oracle requires harness, telemetry, session, models, compat and telemetry-package modules");
	}
	const harness = (await import(pathToFileURL(harnessPath).href)) as AgentHarnessApi;
	const telemetry = (await import(pathToFileURL(telemetryPath).href)) as TelemetryApi;
	const session = (await import(pathToFileURL(sessionPath).href)) as SessionApi;
	const models = (await import(pathToFileURL(modelsPath).href)) as ModelsApi;
	const compat = (await import(pathToFileURL(compatPath).href)) as CompatApi;
	const telemetryPackage = (await import(pathToFileURL(telemetryPackagePath).href)) as TelemetryPackageApi;
	serveRequests({ harness, telemetry, session, models, compat, telemetryPackage });
}

void main().catch((error: unknown) => {
	process.stderr.write(`agent harness oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
