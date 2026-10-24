import { existsSync, mkdirSync, rmSync } from "node:fs";
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
const { archivePersistedSession } = await import(
	pathToFileURL(`${source}/core/session-archive-storage.ts`).href
);
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
		| "new-parent"
		| "restart-notice"
		| "persist-settings"
		| "wait"
		| "switch-archive"
		| "switch-no-db"
		| "probe"
		| "relocate-repair"
		| "switch-trust"
		| "relocate-trust";
	root: string;
	parentSession?: string;
	notice?: string;
	messages?: Record<string, unknown>[];
	jobs?: Record<string, unknown>[];
	args?: Record<string, unknown>;
	handoff?: { sessionFile: string; prompt?: string; oldPid?: number };
	env?: Record<string, string>;
	currentPid?: number;
	parentPid?: number;
	killError?: "OK" | "EPERM" | "ESRCH";
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
		if (request.op === "probe") {
			const previous = process.kill;
			let now = 0;
			let waited = false;
			process.kill = ((..._args: unknown[]) => {
				if (request.killError !== "OK")
					throw Object.assign(new Error("fixture kill failure"), {
						code: request.killError,
					});
				return true;
			}) as typeof process.kill;
			try {
				await waitForSelfRestartParentExit(
					{ sessionFile: "/s", oldPid: 42 },
					{
						currentPid: 100,
						now: () => now,
						sleep: async () => {
							waited = true;
							now = 6000;
						},
					},
				);
			} finally {
				process.kill = previous;
			}
			console.log(JSON.stringify({ waited }));
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
		if (request.op === "switch-trust" || request.op === "relocate-trust") {
			const cwd = join(request.root, "trust-target");
			mkdirSync(cwd);
			const target = SessionManager.create(
				cwd,
				join(request.root, "target-sessions"),
			);
			target.setMetadataControlDbPath(db);
			target.persistForRecovery();
			const order: string[] = [];
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
			};
			runtime._services = { agentDir: request.root, cwd: request.root };
			runtime.emitBeforeSwitch = async () => ({ cancelled: false });
			let capture: unknown;
			let outgoingManagerRelocated = false;
			runtime.teardownCurrent = async () => {
				order.push("shutdown");
				outgoingManagerRelocated = manager.getCwd() === cwd;
			};
			runtime.createRuntime = async (options: {
				projectTrustContext?: { cwd: string; mode: string; hasUI: boolean };
			}) => {
				order.push("factory");
				const context = options.projectTrustContext;
				capture = {
					context: context
						? {
								targetCwd: context.cwd === cwd,
								mode: context.mode,
								hasUI: context.hasUI,
							}
						: null,
					order,
					...(request.op === "relocate-trust"
						? { outgoingManagerRelocated }
						: {}),
				};
				throw new Error("fixture factory stop");
			};
			const context = { cwd, mode: "tui", hasUI: true, ui: {} };
			try {
				if (request.op === "switch-trust")
					await runtime.switchSession(target.getSessionFile(), {
						projectTrustContextFactory: (targetCwd: string) => {
							order.push("trust");
							return { ...context, cwd: targetCwd };
						},
					});
				else await runtime.relocate(cwd, { projectTrustContext: context });
			} catch (error) {
				if (String(error) !== "Error: fixture factory stop") throw error;
			}
			console.log(JSON.stringify(capture));
			continue;
		}
		if (request.op === "relocate-repair") {
			const cwd = join(request.root, "target");
			mkdirSync(cwd);
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
				sendCustomMessage: async () => {},
			};
			runtime._services = { agentDir: request.root, cwd: request.root };
			let repairedBeforeInvalidate = false;
			runtime.teardownCurrent = async () => {
				repairedBeforeInvalidate = process.cwd() === cwd;
			};
			runtime.createRuntime = async () => {
				throw new Error("fixture factory stop");
			};
			await runtime.prepareToolResultRelocation(cwd);
			const original = process.cwd();
			const physical = join(request.root, "deleted");
			mkdirSync(physical);
			process.chdir(physical);
			process.cwd();
			rmSync(physical, { recursive: true });
			try {
				await runtime.activateToolResultRelocation();
			} catch (error) {
				if (String(error) !== "Error: fixture factory stop") throw error;
			} finally {
				process.chdir(original);
			}
			console.log(JSON.stringify({ repairedBeforeInvalidate }));
			continue;
		}
		if (request.op === "switch-archive" || request.op === "switch-no-db") {
			const target = SessionManager.create(
				request.root,
				join(request.root, "target"),
			);
			target.setMetadataControlDbPath(db);
			target.persistForRecovery();
			const plain = target.getSessionFile();
			const archive = archivePersistedSession(db, plain);
			if (request.op === "switch-no-db")
				manager.setMetadataControlDbPath(undefined);
			let capture: unknown;
			const runtime = Object.create(AgentSessionRuntime.prototype);
			runtime.lifecycleTransitionTail = Promise.resolve();
			runtime._session = {
				sessionManager: manager,
				sessionFile: manager.getSessionFile(),
			};
			runtime._services = { agentDir: request.root, cwd: request.root };
			runtime.emitBeforeSwitch = async () => ({ cancelled: false });
			runtime.teardownCurrent = async () => {};
			runtime.createRuntime = async (options: {
				sessionManager: typeof manager;
			}) => {
				capture = {
					restored: options.sessionManager.getSessionFile() === plain,
					metadataBound:
						options.sessionManager.getMetadataControlDbPath() === db,
					plainExists: existsSync(plain),
					archiveExists: existsSync(archive),
				};
				throw new Error("fixture factory stop");
			};
			let failure: string | undefined;
			try {
				await runtime.switchSession(archive);
			} catch (error) {
				failure = String(error);
				if (
					request.op === "switch-archive" &&
					failure !== "Error: fixture factory stop"
				)
					throw error;
			}
			console.log(
				JSON.stringify(
					request.op === "switch-no-db"
						? {
								error: failure,
								plainExists: existsSync(plain),
								archiveExists: existsSync(archive),
							}
						: capture,
				),
			);
			continue;
		}
		if (request.op === "new-metadata" || request.op === "new-parent") {
			let factoryInheritedDb = false;
			let parent: string | null = null;
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
				parent = options.sessionManager.getHeader()?.parentSession ?? null;
				throw new Error("fixture factory stop");
			};
			try {
				await runtime.newSession({ parentSession: request.parentSession });
			} catch (error) {
				if (String(error) !== "Error: fixture factory stop") throw error;
			}
			console.log(
				JSON.stringify(
					request.op === "new-parent" ? { parent } : { factoryInheritedDb },
				),
			);
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
		if (
			request.op === "restart" ||
			request.op === "detach" ||
			request.op === "restart-notice"
		) {
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
			await runtime.restart({ notice: request.notice });
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
				...(request.op === "restart-notice"
					? {
							notices: restored
								.getEntries()
								.filter(
									(entry: { type: string; customType?: string }) =>
										entry.type === "custom_message" &&
										entry.customType === "self_restart",
								)
								.map((entry: { content: unknown }) => entry.content),
						}
					: {}),
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
