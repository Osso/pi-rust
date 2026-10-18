import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fauxAssistantMessage, fauxText, fauxToolCall } from "@earendil-works/pi-ai/compat";

const sdkPath = process.argv[2];
const packagesRoot = resolve(dirname(sdkPath), "../../..");

async function loadModules() {
	const sdkModule = await import(sdkPath);
	const { SessionManager } = await import(`${packagesRoot}/coding-agent/src/core/session-manager.ts`);
	const { SettingsManager } = await import(`${packagesRoot}/coding-agent/src/core/settings-manager.ts`);
	const { AuthStorage } = await import(`${packagesRoot}/coding-agent/src/core/auth-storage.ts`);
	const { ModelRuntime } = await import(`${packagesRoot}/coding-agent/src/core/model-runtime.ts`);
	const { createExtensionRuntime } = await import(`${packagesRoot}/coding-agent/src/core/extensions/index.ts`);
	const { Agent } = await import(`${packagesRoot}/agent/src/agent.ts`);
	const { createFauxCore } = await import(`${packagesRoot}/ai/src/providers/faux.ts`);
	const { AgentSession } = await import(`${packagesRoot}/coding-agent/src/core/agent-session.ts`);
	return {
		sdkModule,
		SessionManager,
		SettingsManager,
		AuthStorage,
		ModelRuntime,
		createExtensionRuntime,
		Agent,
		createFauxCore,
		AgentSession,
	};
}

let modules;

function createResourceLoader() {
	const extensionsResult = { extensions: [], errors: [], runtime: modules.createExtensionRuntime() };
	return {
		getExtensions: () => extensionsResult,
		getSkills: () => ({ skills: [], diagnostics: [] }),
		getPrompts: () => ({ prompts: [], diagnostics: [] }),
		getThemes: () => ({ themes: [], diagnostics: [] }),
		getAgentsFiles: () => ({ agentsFiles: [] }),
		getRulesFiles: () => ({ rulesFiles: [] }),
		getRulesContent: () => undefined,
		getSystemPrompt: () => undefined,
		getSystemPromptSource: () => undefined,
		getAppendSystemPrompt: () => [],
		getAppendSystemPromptSources: () => [],
		extendResources: () => {},
		reload: async () => {},
	};
}

function messageSummary(message) {
	const text = Array.isArray(message.content)
		? message.content.filter((part) => part.type === "text").map((part) => part.text).join("")
		: typeof message.content === "string"
			? message.content
			: "";
	const summary = { role: message.role, text };
	if (message.stopReason !== undefined) summary.stopReason = message.stopReason;
	if (message.errorMessage !== undefined) summary.errorMessage = message.errorMessage;
	return summary;
}

async function run(request) {
	const { SessionManager, SettingsManager, AuthStorage, ModelRuntime, Agent, createFauxCore, AgentSession } = modules;
	const dir = request.dir;
	const agentDir = join(dir, "agent");
	mkdirSync(agentDir, { recursive: true });
	writeFileSync(join(agentDir, "auth.json"), JSON.stringify({ faux: { type: "api_key", key: "faux-key" } }));
	writeFileSync(join(agentDir, "models.json"), JSON.stringify({ providers: {} }));
	const core = createFauxCore({
		api: "faux",
		models: [{ id: "faux-1", name: "Faux Model", reasoning: true, contextWindow: 100000, maxTokens: 4096 }],
		tokenSize: { min: 1, max: 1 },
		tokensPerSecond: 1_000_000,
	});
	core.setResponses([
		fauxAssistantMessage([fauxText("world"), fauxToolCall("end_turn", { reason: "done" })], { stopReason: "toolUse" }),
		fauxAssistantMessage([fauxText("again"), fauxToolCall("end_turn", { reason: "done" })], { stopReason: "toolUse" }),
	]);
	const model = core.getModel();
	const authStorage = AuthStorage.inMemory();
	await authStorage.modify(model.provider, async () => ({ type: "api_key", key: "faux-key" }));
	const modelRuntime = await ModelRuntime.create({
		credentials: authStorage,
		modelsPath: join(dir, "models.json"),
	});
	modelRuntime.registerProvider(model.provider, {
		baseUrl: model.baseUrl,
		api: model.api,
		models: [
			{
				id: model.id,
				name: model.name,
				api: model.api,
				reasoning: model.reasoning,
				input: model.input,
				cost: model.cost,
				contextWindow: model.contextWindow,
				maxTokens: model.maxTokens,
				baseUrl: model.baseUrl,
			},
		],
	});
	const sessionDir = join(dir, "sessions");
	const sessionManager = SessionManager.create(dir, sessionDir);
	const settingsManager = SettingsManager.create(dir, agentDir);
	const agent = new Agent({
		initialState: { systemPrompt: "", model, thinkingLevel: "off", tools: [] },
		streamFn: async (requestModel, context, options) => core.streamSimple(requestModel, context, options),
		sessionId: sessionManager.getSessionId(),
		steeringMode: settingsManager.getSteeringMode(),
		followUpMode: settingsManager.getFollowUpMode(),
		transport: settingsManager.getTransport(),
		thinkingBudgets: settingsManager.getThinkingBudgets(),
		maxRetryDelayMs: settingsManager.getProviderRetrySettings().maxRetryDelayMs,
	});
	const session = new AgentSession({
		agent,
		sessionManager,
		settingsManager,
		cwd: dir,
		agentDir,
		resourceLoader: createResourceLoader(),
		modelRuntime,
		initialActiveToolNames: [],
	});
	const events = [];
	session.subscribe((rawEvent) => {
		const event = structuredClone(rawEvent);
		const entry = { type: event.type };
		if (event.type === "agent_end") entry.willRetry = event.willRetry;
		if (event.type === "thinking_level_changed") entry.level = event.level;
		if (event.type === "message_start" || event.type === "message_update" || event.type === "message_end") {
			entry.message = messageSummary(event.message);
		}
		if (event.type === "message_update") entry.assistantMessageEvent = event.assistantMessageEvent;
		if (event.type === "queue_update") {
			entry.steering = event.steering;
			entry.followUp = event.followUp;
		}
		events.push(entry);
	});
	await session.prompt("hello");
	session.setThinkingLevel("high");
	await session.prompt("second");
	session.setSessionName("Named session");
	const stats = session.getSessionStats();
	const sessionFile = session.sessionFile;
	const lines = readFileSync(sessionFile, "utf8").trim().split("\n").map((line) => JSON.parse(line));
	await session.abort();
	session.dispose();

	return {
		events,
		stats: {
			userMessages: stats.userMessages,
			assistantMessages: stats.assistantMessages,
			toolCalls: stats.toolCalls,
			toolResults: stats.toolResults,
			totalMessages: stats.totalMessages,
		},
		messages: sessionManager.buildSessionContext().messages.map(messageSummary),
		lines,
		dir,
	};
}

async function main() {
	modules = await loadModules();
	process.stdin.setEncoding("utf8");
	let buffer = "";
	for await (const chunk of process.stdin) {
		buffer += chunk;
		let index;
		while ((index = buffer.indexOf("\n")) !== -1) {
			const line = buffer.slice(0, index);
			buffer = buffer.slice(index + 1);
			if (!line.trim()) continue;
			const request = JSON.parse(line);
			try {
				const value = await run(request);
				process.stdout.write(`${JSON.stringify({ ok: true, value })}\n`);
			} catch (error) {
				process.stdout.write(`${JSON.stringify({ ok: false, error: String(error?.stack ?? error) })}\n`);
			}
		}
	}
}

void main();
