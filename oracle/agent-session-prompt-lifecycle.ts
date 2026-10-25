import { dirname, join, resolve } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai/compat";

async function persistSteeredSession(context) {
	const { session, sessionManager, dir } = context;
	const root = resolve(dirname(process.argv[2]), "../../..");
	const { MultiAgentStore } = await import(
		`${root}/coding-agent/src/core/multi-agent-store.ts`
	);
	const lifecycle = await import(
		`${root}/coding-agent/extensions/agents-core/src/lifecycle-runtime.ts`
	);
	const runtime = await import(
		`${root}/coding-agent/extensions/agents-core/src/runtime.ts`
	);
	const db = await import(
		`${root}/coding-agent/src/core/session-control-db.ts`
	);
	const controlDbPath = join(dir, "control.db");
	sessionManager.setMetadataControlDbPath(controlDbPath);
	sessionManager.persistForRecovery();
	const store = new MultiAgentStore();
	store.setPersistenceSessionManager(sessionManager);
	// Configure the same worker/store fields as the native SDK fixture.
	session._multiAgentStore = store;
	session._multiAgentAgentId = "agent_1";
	session._multiAgentParentSessionId = sessionManager.getSessionId();
	await session.bindExtensions({ controlDbPath });
	session.setActiveToolsByName([]);
	const coordinator = lifecycle.createLifecycleCoordinator(store);
	const prepared = coordinator.prepareChild({
		agentId: "agent_1",
		parentId: "main",
		displayName: "Agent agent_1",
		agentType: "worker",
		cwd: dir,
		permission: { policy: "on-request", narrowed: true },
		transcript: {
			sessionId: sessionManager.getSessionId(),
			path: sessionManager.getSessionFile(),
		},
	});
	const committed = coordinator.commitRunningChild(
		prepared,
		sessionManager.getSessionId(),
	);
	if (!committed.ok)
		throw new Error(`source child creation: ${committed.error}`);
	store.publishLifecycleCoordinatorSnapshot(committed.agent);
	const sender = { agentId: null, sessionId: sessionManager.getSessionId() };
	db.registerRuntimeMailboxListener(
		controlDbPath,
		sender,
		process.pid,
		sessionManager.getSessionFile(),
	);
	const steered = runtime.requestAgentSteering(
		store,
		{ agentId: "agent_1", message: "Continue verification" },
		{
			actorAgentId: null,
			controlDbPath,
			sessionId: sessionManager.getSessionId(),
		},
	);
	if (!steered.ok) throw new Error(`source steering: ${steered.error}`);
	db.enqueueRuntimeMailboxMessage(controlDbPath, {
		kind: "steer",
		recipient: { agentId: "agent_1", sessionId: sessionManager.getSessionId() },
		sender,
		storeRef: {
			messageId: steered.message.id,
			sessionPath: sessionManager.getSessionFile(),
		},
	});
	return store;
}

export async function runPromptLifecycleScenario(context) {
	const { session, core } = context;
	try {
		const store = await persistSteeredSession(context);
		let started;
		let release;
		const gate = new Promise((resolve) => {
			release = resolve;
		});
		const pending = new Promise((resolve) => {
			started = resolve;
		});
		core.setResponses([
			fauxAssistantMessage("initial reply"),
			async () => {
				started();
				await gate;
				return fauxAssistantMessage("steer done");
			},
			fauxAssistantMessage("follow-up done"),
		]);
		const turn = session.prompt("hello");
		await pending;
		await session.prompt("additional input", { streamingBehavior: "followUp" });
		const whileRunning = store.getAgent("agent_1").lifecycle;
		release();
		await turn;
		const completed = store.getAgent("agent_1");
		return {
			whileRunning,
			finalLifecycle: completed.lifecycle,
			summary: completed.result?.summary,
		};
	} finally {
		session.dispose();
	}
}
