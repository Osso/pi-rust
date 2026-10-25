import { readFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const supervisorMainPath = process.argv[2];
const packagesRoot = resolve(dirname(supervisorMainPath), "../../..");

type Request = { id: string | number; operation: string; dir: string };

function createResourceLoader(runtime: unknown) {
	return {
		getExtensions: () => ({ extensions: [], errors: [], runtime }),
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

async function loadModules() {
	const main = await import(supervisorMainPath);
	const { SessionManager } = await import(
		`${packagesRoot}/coding-agent/src/core/session-manager.ts`
	);
	const { AuthStorage } = await import(
		`${packagesRoot}/coding-agent/src/core/auth-storage.ts`
	);
	const { ModelRegistry } = await import(
		`${packagesRoot}/coding-agent/src/core/model-registry.ts`
	);
	const { createAgentSession } = await import(
		`${packagesRoot}/coding-agent/src/core/sdk.ts`
	);
	const { createSupervisorResponseTool } = await import(
		`${packagesRoot}/coding-agent/src/supervisor/response-tool.ts`
	);
	const controlDb = await import(
		`${packagesRoot}/coding-agent/src/core/session-control-db.ts`
	);
	const sqlite = await import(
		`${packagesRoot}/coding-agent/src/core/sqlite.ts`
	);
	const { registerFauxProvider, fauxAssistantMessage, fauxToolCall } =
		await import(`${packagesRoot}/ai/src/compat.ts`);
	const { createExtensionRuntime } = await import(
		`${packagesRoot}/coding-agent/src/core/extensions/index.ts`
	);
	return {
		main,
		SessionManager,
		AuthStorage,
		ModelRegistry,
		createAgentSession,
		createSupervisorResponseTool,
		controlDb,
		sqlite,
		registerFauxProvider,
		fauxAssistantMessage,
		fauxToolCall,
		createExtensionRuntime,
	};
}

function normalizeSessionFile(text: string): unknown[] {
	return text
		.split("\n")
		.filter((line) => line.length > 0)
		.map((line) => {
			const value = JSON.parse(line) as Record<string, unknown>;
			const normalize = (node: unknown): unknown => {
				if (Array.isArray(node)) return node.map(normalize);
				if (typeof node !== "object" || node === null) return node;
				let object = node as Record<string, unknown>;
				if (object.customType === "codex_restriction_context") {
					const data = object.data as Record<string, unknown>;
					const requestId = data?.requestId;
					const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
					const validUuid = typeof requestId === "string" && requestId.length === 36 && uuid.test(requestId);
					if (!validUuid) {
						throw new Error("restriction context requestId must be a v4 UUID");
					}
					object = { ...object, data: { ...data, requestId: "<requestId>" } };
				}
				const result: Record<string, unknown> = {};
				for (const [key, child] of Object.entries(object)) {
					if (key === "id" || key === "parentId" || key === "toolCallId")
						result[key] = "<id>";
					else if (key === "timestamp") result[key] = "<timestamp>";
					else if (key === "cwd") result[key] = "<cwd>";
					else if (
						key === "input" ||
						key === "output" ||
						key === "cacheRead" ||
						key === "cacheWrite" ||
						key === "totalTokens"
					)
						result[key] = "<number>";
					else result[key] = normalize(child);
				}
				return result;
			};
			return normalize(value);
		});
}

function dumpControlDb(
	controlDbPath: string,
	sqlite: {
		createSqliteDatabase: (path: string) => {
			prepare: (sql: string) => { all: () => unknown[] };
			close: () => void;
		};
	},
) {
	const db = sqlite.createSqliteDatabase(controlDbPath);
	try {
		return db
			.prepare(
				"SELECT id, kind, status, response_json FROM supervisor_requests ORDER BY id",
			)
			.all() as unknown[];
	} finally {
		db.close();
	}
}

async function run(request: Request) {
	const {
		main,
		SessionManager,
		AuthStorage,
		ModelRegistry,
		createAgentSession,
		createSupervisorResponseTool,
		controlDb,
		sqlite,
		registerFauxProvider,
		fauxAssistantMessage,
		fauxToolCall,
		createExtensionRuntime,
	} = await loadModules();
	const dir = request.dir;
	mkdirSync(dir, { recursive: true });
	const controlDbPath = join(dir, "control.sqlite");
	const sessionDir = join(dir, "sessions");
	const faux = registerFauxProvider({
		api: "faux",
		models: [
			{
				id: "gpt-6.1-sol",
				name: "Sol",
				reasoning: true,
				contextWindow: 200000,
				maxTokens: 8192,
			},
		],
		provider: "openai-codex",
		tokenSize: { max: 1, min: 1 },
		tokensPerSecond: 1_000_000,
	});
	faux.setResponses([
		fauxAssistantMessage(
			[fauxToolCall("supervisor_response", { kind: "approve", reason: "ok" })],
			{
				stopReason: "toolUse",
			},
		),
	]);
	const authStorage = AuthStorage.inMemory();
	authStorage.set(faux.getModel().provider, {
		type: "api_key",
		key: "faux-key",
	});
	const modelRegistry = ModelRegistry.inMemory(authStorage);
	const model = faux.getModel();
	modelRegistry.registerProvider(model.provider, {
		api: model.api,
		apiKey: "faux-key",
		baseUrl: model.baseUrl,
		models: [
			{
				api: model.api,
				baseUrl: model.baseUrl,
				cost: model.cost,
				contextWindow: model.contextWindow,
				id: model.id,
				input: model.input,
				maxTokens: model.maxTokens,
				name: model.name,
				reasoning: model.reasoning,
			},
		],
	});
	const sessionManager = SessionManager.create(dir, sessionDir, {
		id: "supervisor",
	});
	sessionManager.setMetadataControlDbPath(controlDbPath, {
		indexMessageText: false,
	});
	// Mirror frozen main.ts:createSupervisorAgentSession through public SDK dependencies.
	const { session } = await createAgentSession({
		agentDir: dir,
		authStorage,
		cwd: dir,
		disableRuntimeCoordinationInbound: true,
		excludeTools: main.SUPERVISOR_EXCLUDED_TOOL_NAMES,
		model,
		modelRegistry,
		resourceLoader: createResourceLoader(createExtensionRuntime()),
		sessionManager,
		settingsManager: main.createSupervisorSettingsManager(),
		thinkingLevel: "low",
		customTools: [createSupervisorResponseTool()],
		tools: [...main.SUPERVISOR_TOOL_NAMES, "supervisor_response"],
	});
	controlDb.postSupervisorRequest(controlDbPath, {
		deadlineAt: "2099-01-01T00:00:00.000Z",
		kind: "approval_review",
		payload: { toolName: "read" },
		projectId: "pi",
		senderSessionId: "requester",
	});
	const controller = new AbortController();
	await main.runSupervisorRequestLoop({
		claimToken: "claim-token",
		consolePrompts: { enqueue: () => {}, take: () => undefined },
		controlDbPath,
		processRequest: async (
			dbPath: string,
			supervisorRequest: unknown,
			activeSession: unknown,
		) => {
			await main.processSupervisorRequest(
				dbPath,
				supervisorRequest,
				activeSession,
			);
			controller.abort();
		},
		session,
		signal: controller.signal,
		wakeServer: {
			currentGeneration: () => 0,
			waitForWakeAfter: async () => {
				await new Promise((resolvePromise) => setTimeout(resolvePromise, 5));
			},
		},
	});
	const sessionFile = sessionManager.getSessionFile();
	return {
		controlRows: dumpControlDb(
			controlDbPath,
			sqlite as unknown as {
				createSqliteDatabase: (path: string) => {
					prepare: (sql: string) => { all: () => unknown[] };
					close: () => void;
				};
			},
		),
		session: sessionFile
			? normalizeSessionFile(readFileSync(sessionFile, "utf8"))
			: [],
	};
}

let buffer = "";
process.stdin.on("data", (chunk) => {
	buffer += String(chunk);
	while (true) {
		const newline = buffer.indexOf("\n");
		if (newline === -1) return;
		const line = buffer.slice(0, newline);
		buffer = buffer.slice(newline + 1);
		const request = JSON.parse(line) as Request;
		run(request)
			.then((output) => {
				process.stdout.write(
					`${JSON.stringify({ id: request.id, ok: true, output })}\n`,
				);
			})
			.catch((error: unknown) => {
				const message = error instanceof Error ? error.message : String(error);
				process.stdout.write(
					`${JSON.stringify({ id: request.id, ok: false, error: { message } })}\n`,
				);
			});
	}
});
