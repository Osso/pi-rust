import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { installAssistantSnapshots } from "./assistant-snapshots.ts";
import { fauxAssistantMessage, fauxText, fauxToolCall } from "@earendil-works/pi-ai/compat";

const sdkPath = process.argv[2];
const packagesRoot = resolve(dirname(sdkPath), "../../..");

async function loadModules() {
	await installAssistantSnapshots(`${packagesRoot}/ai/src`);
	const sdkModule = await import(sdkPath);
	const { SessionManager } = await import(`${packagesRoot}/coding-agent/src/core/session-manager.ts`);
	const { SettingsManager } = await import(`${packagesRoot}/coding-agent/src/core/settings-manager.ts`);
	const { AuthStorage } = await import(`${packagesRoot}/coding-agent/src/core/auth-storage.ts`);
	const { ModelRegistry } = await import(`${packagesRoot}/coding-agent/src/core/model-registry.ts`);
	const { createExtensionRuntime } = await import(`${packagesRoot}/coding-agent/src/core/extensions/index.ts`);
	const { Agent } = await import(`${packagesRoot}/agent-core/src/index.ts`);
	const { createFauxCore } = await import(`${packagesRoot}/ai/src/providers/faux.ts`);
	const { AgentSession } = await import(`${packagesRoot}/coding-agent/src/core/agent-session.ts`);
	return {
		sdkModule,
		SessionManager,
		SettingsManager,
		AuthStorage,
		ModelRegistry,
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
		getSystemPrompt: () => "Differential system prompt",
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
	const { SessionManager, SettingsManager, AuthStorage, ModelRegistry, Agent, createFauxCore, AgentSession } = modules;
	const dir = request.dir;
	const agentDir = join(dir, "agent");
	mkdirSync(agentDir, { recursive: true });
	writeFileSync(join(agentDir, "auth.json"), JSON.stringify({ faux: { type: "api_key", key: "faux-key" } }));
	writeFileSync(join(agentDir, "models.json"), JSON.stringify({ providers: {} }));
	const core = createFauxCore({
		provider: request.operation === "quota-fallback" ? "openai-codex" : "faux",
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
	authStorage.set(model.provider, { type: "api_key", key: "faux-key" });
	const modelRegistry = ModelRegistry.create(authStorage, join(dir, "models.json"));
	modelRegistry.registerProvider(model.provider, {
		apiKey: "faux-key",
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
	if (request.operation === "quota-fallback") {
		for (const provider of ["openai-codex-gc", "openai-codex-team"]) {
			authStorage.set(provider, { type: "api_key", key: "faux-key" });
			modelRegistry.registerProvider(provider, {
				apiKey: "faux-key", baseUrl: model.baseUrl, api: model.api,
				models: [{ ...model, provider }],
			});
		}

	}
	const sessionDir = join(dir, "sessions");
	const sessionManager = SessionManager.create(dir, sessionDir);
	const settingsManager = ["preflight-compaction", "compaction-lock"].includes(request.operation)
		? SettingsManager.inMemory({ compaction: { enabled: false, keepRecentTokens: 1 } })
		: SettingsManager.create(dir, agentDir);
	const requestedProviders = [];
	if (request.operation === "quota-fallback") settingsManager.setRetryEnabled(false);
	const agent = new Agent({
		initialState: { systemPrompt: "", model, thinkingLevel: "off", tools: [] },
		streamFn: (requestModel, context, options) => {
			requestedProviders.push(requestModel.provider);
			if (request.operation === "quota-fallback") {
				const account = createFauxCore({ provider: requestModel.provider, api: "faux" });
				account.setResponses([fauxAssistantMessage([], { stopReason: "error", errorMessage: "usage limit" })]);
				return account.streamSimple(requestModel, context, options);
			}
			return core.streamSimple(requestModel, context, options);
		},
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
		modelRegistry,
		initialActiveToolNames: [],
	});
	if (request.operation === "export-error") {
		try {
			session.exportToJsonl(request.outputPath);
			return { success: true };
		} catch (error) {
			return { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) };
		} finally {
			session.dispose();
		}
	}
	if (request.operation === "navigation-empty-summary") {
		const first = sessionManager.appendMessage({ role: "user", content: "first request", timestamp: 0 });
		sessionManager.appendMessage(fauxAssistantMessage("old answer"));
		agent.state.messages = sessionManager.buildSessionContext().messages;
		const runner = session.extensionRunner;
		const emit = runner.emit.bind(runner);
		runner.hasHandlers = (type) => type === "session_before_tree";
		runner.emit = async (event) => event.type === "session_before_tree" ? { summary: { summary: "" }, label: "" } : emit(event);
		const result = await session.navigateTree(first, { summarize: true, label: "" });
		const entries = sessionManager.getEntries();
		session.dispose();
		return {
			hasSummary: result.summaryEntry !== undefined,
			atRoot: sessionManager.getLeafId() === null,
			summaries: entries.filter((entry) => entry.type === "branch_summary").length,
			labels: entries.filter((entry) => entry.type === "label").length,
		};
	}
	if (request.operation === "navigation-auth") {
		const first = sessionManager.appendMessage({ role: "user", content: "first request", timestamp: 0 });
		sessionManager.appendMessage(fauxAssistantMessage("old answer"));
		agent.state.model = { ...model, provider: "unconfigured-provider" };
		agent.state.messages = sessionManager.buildSessionContext().messages;
		try {
			await session.navigateTree(first, { summarize: true });
			return { success: true };
		} catch (error) {
			return { error: error.message };
		} finally {
			session.dispose();
		}
	}
	if (["preflight-compaction", "compaction-lock"].includes(request.operation)) {
		sessionManager.appendMessage({ role: "user", content: "older message to compact", timestamp: 0 });
		sessionManager.appendMessage(fauxAssistantMessage("assistant response to compact"));
		for (let i = 0; i < 4; i++) {
			sessionManager.appendMessage({ role: "user", content: `newer request ${i}`, timestamp: i + 1 });
		}
		sessionManager.appendMessage(fauxAssistantMessage("latest answer"));
		agent.state.messages = sessionManager.buildSessionContext().messages;
		settingsManager.setCompactionEnabled(false);
		const runner = session.extensionRunner;
		const emit = runner.emit.bind(runner);
		if (request.operation === "compaction-lock") {
			sessionManager.appendMessage({ role: "user", content: "unfinished request", timestamp: 10 });
			agent.state.messages = sessionManager.buildSessionContext().messages;
			settingsManager.setRetryEnabled(false);
			runner.hasHandlers = (type) => type === "compaction";
			runner.emit = async (event) => event.type === "compaction" ? {
				compaction: { summary: "saved summary", firstKeptEntryId: event.preparation.firstKeptEntryId, tokensBefore: 1 },
			} : emit(event);
			let release;
			let markStarted;
			const gate = new Promise((resolve) => { release = resolve; });
			const started = new Promise((resolve) => { markStarted = resolve; });
			core.setResponses([async () => {
				markStarted();
				await gate;
				return fauxAssistantMessage("resumed answer");
			}]);
			const compacting = session.compact();
			await started;
			const accepted = await Promise.race([
				session.prompt("new input", { streamingBehavior: "followUp" }).then(() => true),
				new Promise((resolve) => setTimeout(() => resolve(false), 100)),
			]);
			release();
			await session.abort();
			await compacting;
			session.dispose();
			return accepted;
		}
		runner.hasHandlers = (type) => type === "session_before_compact";
		runner.emit = async (event) => event.type === "session_before_compact" ? {
			compaction: { summary: "obsolete preflight summary", firstKeptEntryId: event.preparation.firstKeptEntryId, tokensBefore: 1 },
		} : emit(event);
		try {
			const result = await session.compact("keep decisions");
			return { summary: result.summary };
		} catch (error) {
			return { error: error.message };
		} finally {
			session.dispose();
		}
	}
	if (["idle-steer", "navigation-streaming"].includes(request.operation)) {
		sessionManager.appendMessage({ role: "user", content: "earlier request", timestamp: 0 });
		agent.state.messages = sessionManager.buildSessionContext().messages;
		let release;
		let markStarted;
		const gate = new Promise((resolve) => { release = resolve; });
		const started = new Promise((resolve) => { markStarted = resolve; });
		core.setResponses([async () => {
			markStarted();
			await gate;
			return fauxAssistantMessage("steered");
		}]);
		const steering = session.steer("idle steer");
		await started;
		let navigation;
		if (request.operation === "navigation-streaming") {
			try {
				navigation = await session.navigateTree(sessionManager.getLeafId());
			} catch (error) {
				navigation = { error: error.message };
			}
		}
		const returnedBeforeResponse = await Promise.race([
			steering.then(() => true), new Promise((resolve) => setTimeout(() => resolve(false), 100)),
		]);
		release();
		await steering;
		await session.abort();
		session.dispose();
		return request.operation === "navigation-streaming" ? navigation : returnedBeforeResponse;
	}
	if (request.operation === "quota-fallback") {
		await session.prompt("quota test");
		const result = { requestedProviders, provider: session.model.provider };
		session.dispose();
		return result;
	}
	if (request.operation === "cycle-models") {
		session.setScopedModels(request.scopes.map((scope) => ({
			model: { ...model, ...scope },
			thinkingLevel: "high",
		})));
		const results = [];
		for (const direction of request.directions) {
			const result = await session.cycleModel(direction);
			results.push(result ? {
				id: result.model.id,
				name: result.model.name,
				thinkingLevel: result.thinkingLevel,
				isScoped: result.isScoped,
			} : null);
		}
		session.dispose();
		return results;
	}
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
	const cycle = await session.cycleModel();
	const contextUsage = session.getContextUsage();
	const forking = session.getUserMessagesForForking();
	session.recordBashResult("echo hi", {
		output: "hi",
		exitCode: 0,
		cancelled: false,
		truncated: false,
	});
	const statsAfterBash = session.getSessionStats();
	const exportPath = session.exportToJsonl(join(dir, "export.jsonl"));
	const exportLines = readFileSync(exportPath, "utf8").trim().split("\n").map((line) => JSON.parse(line));
	const firstUserEntry = sessionManager
		.getEntries()
		.find((entry) => entry.type === "message" && entry.message.role === "user");
	const navigation = await session.navigateTree(firstUserEntry.id, { summarize: false });
	const stats = session.getSessionStats();
	const sessionFile = session.sessionFile;
	const lines = readFileSync(sessionFile, "utf8").trim().split("\n").map((line) => JSON.parse(line));
	session.setSessionName("Named session");
	await session.abort();
	session.dispose();

	return {
		events,
		cycle: cycle ?? null,
		contextUsage: contextUsage ?? null,
		forking,
		navigation,
		exportLines,
		statsAfterBash: {
			userMessages: statsAfterBash.userMessages,
			assistantMessages: statsAfterBash.assistantMessages,
			toolCalls: statsAfterBash.toolCalls,
			toolResults: statsAfterBash.toolResults,
			totalMessages: statsAfterBash.totalMessages,
		},
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
