import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const {
	applySelfRestartRequest,
	consumeSelfRestartRequest,
	resolveRestartInterruptedToolCalls,
	waitForSelfRestartParentExit,
} = await import(pathToFileURL(`${source}/core/self-restart.ts`).href);
const { ToolDetachRegistry } = await import(
	pathToFileURL(`${source}/core/tool-detach-registry.ts`).href
);
const { AgentSession } = await import(
	pathToFileURL(`${source}/core/agent-session.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
const { readMultiAgentState, readSessionMetadata } = await import(
	pathToFileURL(`${source}/core/session-control-db.ts`).href
);
const { createSqliteDatabase } = await import(
	pathToFileURL(`${source}/core/sqlite.ts`).href
);

interface Request {
	op:
		| "apply"
		| "consume"
		| "resolve"
		| "restart"
		| "detach"
		| "new-metadata"
		| "persist-settings"
		| "wait";
	root: string;
	messages?: Record<string, unknown>[];
	jobs?: Record<string, unknown>[];
	args?: Record<string, unknown>;
	handoff?: { sessionFile: string; prompt?: string; oldPid?: number };
	env?: Record<string, string>;
	currentPid?: number;
	parentPid?: number;
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		if (request.op === "apply") {
			const args = request.args ?? {};
			applySelfRestartRequest(args, request.handoff);
			console.log(JSON.stringify({ args }));
			continue;
		}
		if (request.op === "wait") {
			const calls: string[] = [];
			await waitForSelfRestartParentExit(request.handoff, {
				currentPid: request.currentPid,
				isProcessAlive: (pid: number) => {
					calls.push(`alive:${pid}`);
					return false;
				},
				now: () => {
					calls.push("now");
					return 1000;
				},
				sleep: async () => {
					calls.push("sleep");
				},
			});
			console.log(JSON.stringify({ calls }));
			continue;
		}
		if (request.op === "consume") {
			const env = request.env ?? {};
			const handoff = consumeSelfRestartRequest(
				env,
				request.currentPid,
				request.parentPid,
			);
			console.log(JSON.stringify({ handoff: handoff ?? null, env }));
			continue;
		}
		const manager = SessionManager.create(
			request.root,
			join(request.root, "sessions"),
		);
		const db = join(request.root, "control.db");
		manager.setMetadataControlDbPath(db);
		readMultiAgentState(db, manager.getSessionFile());
		const connection = createSqliteDatabase(db);
		try {
			for (const [index, job] of (request.jobs ?? []).entries())
				connection
					.prepare(
						"INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,?,?,'fixture')",
					)
					.run(manager.getSessionFile(), `job-${index}`, JSON.stringify(job));
		} finally {
			connection.close();
		}
		for (const message of request.messages ?? [])
			manager.appendMessage(message);
		if (request.op === "new-metadata") {
			let factoryInheritedDb = false;
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
				cwd: request.root,
			};
			runtime._services = { agentDir: request.root, cwd: request.root };
			runtime.emitBeforeSwitch = async () => ({ cancelled: false });
			runtime.teardownCurrent = async () => {};
			runtime.createRuntime = async (options: {
				sessionManager: typeof manager;
			}) => {
				factoryInheritedDb =
					options.sessionManager.getMetadataControlDbPath() === db;
				throw new Error("fixture factory stop");
			};
			try {
				await runtime.newSession();
			} catch (error) {
				if (String(error) !== "Error: fixture factory stop") throw error;
			}
			console.log(JSON.stringify({ factoryInheritedDb }));
			continue;
		}
		if (request.op === "persist-settings") {
			let persisted: unknown;
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
				model: { provider: "faux", id: "faux-1" },
				thinkingLevel: "off",
				detachAllRunningTools: () => {},
			};
			runtime.beforeProcessRestart = async () => {
				const row = readSessionMetadata(db, manager.getSessionFile());
				persisted = {
					provider: row?.modelProvider,
					model: row?.modelId,
					thinking: row?.thinkingLevel,
				};
			};
			runtime.teardownCurrent = async () => {};
			runtime.processRestarter = async () => {
				throw new Error("fixture restarter stop");
			};
			try {
				await runtime.restart({ process: true });
			} catch (error) {
				if (String(error) !== "Error: fixture restarter stop") throw error;
			}
			console.log(JSON.stringify({ persisted }));
			continue;
		}
		const start = Date.now();
		let restored = manager;
		const detachOrder: number[] = [];
		const registry = new ToolDetachRegistry();
		const unregister = [0, 1, 2].map((index) =>
			registry.register({
				detach: () => {
					detachOrder.push(index);
					return index !== 0;
				},
			}),
		);
		if (request.op === "restart" || request.op === "detach") {
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
				_toolDetachRegistry: registry,
				detachAllRunningTools: AgentSession.prototype.detachAllRunningTools,
			};
			runtime._services = { agentDir: request.root };
			runtime.teardownCurrent = async () => {};
			runtime.createRuntime = async (options: { sessionManager: unknown }) => ({
				session: { sessionManager: options.sessionManager },
			});
			runtime.apply = (result: { session: unknown }) => {
				runtime._session = result.session;
			};
			runtime.finishSessionReplacement = async () => {};
			await runtime.restart();
			restored = runtime.session.sessionManager;
		} else {
			resolveRestartInterruptedToolCalls(manager);
		}
		for (const dispose of unregister) dispose();
		const end = Date.now();
		const results = restored
			.buildSessionContext()
			.messages.filter(
				(message: { role: string }) => message.role === "toolResult",
			);
		console.log(
			JSON.stringify({
				...(request.op === "detach" ? { detachOrder } : {}),
				results: results.map(
					({ timestamp, ...result }: { timestamp: number }) => result,
				),
				timestampsValid: results.every(
					({ timestamp }: { timestamp: number }) =>
						timestamp >= start && timestamp <= end,
				),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ error: String(error) }));
	}
}
