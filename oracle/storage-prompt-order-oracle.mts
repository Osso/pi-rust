import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { DatabaseSync } from "node:sqlite";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
const packages = resolve(root, "../../..");
const load = (path: string) => import(pathToFileURL(path).href);
const { Agent } = await load(join(packages, "agent-core/src/index.ts"));
const { createFauxCore, fauxAssistantMessage } = await load(
	join(packages, "ai/src/providers/faux.ts"),
);
const { AgentSession } = await load(join(root, "agent-session.ts"));
const { LifecycleCoordinator } = await load(
	join(root, "lifecycle-coordinator.ts"),
);
const { SessionManager } = await load(join(root, "session-manager.ts"));
const { SettingsManager } = await load(join(root, "settings-manager.ts"));
const { AuthStorage } = await load(join(root, "auth-storage.ts"));
const { ModelRegistry } = await load(join(root, "model-registry.ts"));
const { MultiAgentStore } = await load(join(root, "multi-agent-store.ts"));
const { createEventBus } = await load(join(root, "event-bus.ts"));
const { createExtensionRuntime, loadExtensionFromFactory } = await load(
	join(root, "extensions/index.ts"),
);

async function observe(dir: string, operation?: string): Promise<unknown> {
	const database = join(dir, "control.sqlite");
	const manager = SessionManager.create(dir, join(dir, "sessions"));
	manager.setMetadataControlDbPath(database);
	manager.persistForRecovery();
	const store = new MultiAgentStore();
	store.setPersistenceSessionManager(manager);
	const core = createFauxCore({
		api: "faux",
		models: [
			{ id: "faux-1", name: "Faux", contextWindow: 100000, maxTokens: 4096 },
		],
		tokensPerSecond: 1_000_000,
	});
	core.setResponses([fauxAssistantMessage("done")]);
	const model = core.getModel();
	const auth = AuthStorage.inMemory();
	auth.set(model.provider, { type: "api_key", key: "private-fixture" });
	const registry = ModelRegistry.create(auth, join(dir, "models.json"));
	registry.registerProvider(model.provider, {
		apiKey: "private-fixture",
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
	const events: string[] = [];
	const runtime = createExtensionRuntime();
	const extension = await loadExtensionFromFactory(
		(api: {
			registerCommand(
				name: string,
				options: { handler: () => Promise<void> },
			): void;
		}) => {
			api.registerCommand("ordercmd", {
				handler: async () => {
					events.push("command");
				},
			});
		},
		dir,
		createEventBus(),
		runtime,
	);
	const resourceLoader = {
		getExtensions: () => ({ extensions: [extension], errors: [], runtime }),
		getSkills: () => ({ skills: [], diagnostics: [] }),
		getPrompts: () => ({ prompts: [], diagnostics: [] }),
		getThemes: () => ({ themes: [], diagnostics: [] }),
		getAgentsFiles: () => ({ agentsFiles: [] }),
		getRulesFiles: () => ({ rulesFiles: [] }),
		getRulesContent: () => undefined,
		getSystemPrompt: () => "",
		getSystemPromptSource: () => undefined,
		getAppendSystemPrompt: () => [],
		getAppendSystemPromptSources: () => [],
		extendResources: () => {},
		reload: async () => {},
	};
	const session = new AgentSession({
		agent: new Agent({
			initialState: {
				model,
				systemPrompt: "",
				thinkingLevel: "off",
				tools: [],
			},
			streamFn: (requestModel: unknown, context: unknown, options: unknown) =>
				core.streamSimple(requestModel, context, options),
		}),
		sessionManager: manager,
		settingsManager: SettingsManager.inMemory(),
		cwd: dir,
		agentDir: dir,
		resourceLoader,
		modelRegistry: registry,
		initialActiveToolNames: [],
		multiAgentStore: store,
	});
	const db = new DatabaseSync(database);
	if (operation === "finalizeFailure") {
		const coordinator = new LifecycleCoordinator({
			controlDbPath: database,
			createAgentId: () => "agent_1",
			now: () => new Date().toISOString(),
			processIdentity: session._detachedJobProcessIdentity,
			sessionPath: manager.getSessionFile(),
		});
		const prepared = coordinator.prepareChild({
			agentId: "agent_1",
			parentId: "main",
			displayName: "Agent agent_1",
			agentType: "worker",
			cwd: dir,
			permission: { policy: "on-request", narrowed: true },
		});
		const committed = coordinator.commitRunningChild(
			prepared,
			manager.getSessionId(),
		);
		if (!committed.ok) throw new Error("fixture child rejected");
		store.publishLifecycleCoordinatorSnapshot(committed.agent);
		db.exec(
			"CREATE TRIGGER fail_outbox_claim BEFORE UPDATE ON multi_agent_terminal_outbox WHEN NEW.status='claimed' BEGIN SELECT RAISE(ABORT,'fixture claim failure'); END",
		);
		try {
			session._finalizeReservedMultiAgent(committed.agent, "completed", {
				result: { summary: "completed" },
			});
			return { threw: false };
		} catch (error) {
			return {
				threw: true,
				message: error instanceof Error ? error.message : String(error),
				status: db
					.prepare(
						"SELECT status FROM multi_agent_terminal_outbox WHERE agent_id='agent_1'",
					)
					.get()?.status,
			};
		} finally {
			session.dispose();
			db.close();
		}
	}
	db.prepare(
		"INSERT INTO multi_agent_terminal_outbox(session_path,agent_id,terminal_revision,event_kind,status,attempt_count,updated_at) VALUES (?,'missing',2,'completed','pending',0,?)",
	).run(manager.getSessionFile(), new Date().toISOString());
	const attempts = (): number => {
		const row = db
			.prepare(
				"SELECT attempt_count FROM multi_agent_terminal_outbox WHERE agent_id='missing'",
			)
			.get();
		if (typeof row?.attempt_count !== "number") {
			throw new Error(
				"fixture terminal outbox row missing numeric attempt count",
			);
		}
		return row.attempt_count;
	};
	let atStart: number | undefined;
	session.subscribe((event: { type: string }) => {
		if (event.type === "agent_start") {
			events.push("prompt");
			atStart = attempts();
		}
	});
	try {
		await session.prompt("/ordercmd");
		const afterCommand = attempts();
		await session.prompt("hello");
		const afterPrompt = attempts();
		return { afterCommand, atStart, afterPrompt, events };
	} finally {
		session.dispose();
		db.close();
	}
}
for await (const line of createInterface({ input: process.stdin })) {
	const request: { id: number; dir: string; operation?: string } =
		JSON.parse(line);
	try {
		console.log(
			JSON.stringify({
				id: request.id,
				ok: true,
				output: await observe(request.dir, request.operation),
			}),
		);
	} catch (error) {
		console.log(
			JSON.stringify({ id: request.id, ok: false, error: String(error) }),
		);
	}
}
