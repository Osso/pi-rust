import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	if (request.operation === "clientScenarios") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const { existsSync } = await import("node:fs");
		const db = await import(pathToFileURL(join(dirname(process.argv[2]), "..", "core", "session-control-db.ts")).href);
		const results = [];
		for (const raw of requireCases(request)) {
			const input = raw as { scenario: string; db: string };
			const controller = new AbortController();
			let starts = 0;
			if (input.scenario === "preabort") controller.abort();
			const response = await (api.requestSupervisorDecision as (...args: unknown[]) => Promise<unknown>)(
				{
					controlDbPath: input.db,
					kind: "approval_review",
					payload: { toolName: "read" },
					projectId: "pi",
					senderSessionId: "main",
					timeoutMs: input.scenario === "pollabort" || input.scenario === "complete" ? 10000 : 0,
					maxAttempts:
						input.scenario === "zero" ? 0 : input.scenario === "retryabort" || input.scenario === "timeout" ? 2 : 1,
					pollIntervalMs: input.scenario === "pollabort" ? 5000 : 1,
					retryDelayMs: input.scenario === "retryabort" ? 100000 : 0,
					retryJitterRatio: 0,
					signal: controller.signal,
				},
				{
					ensureRunning: async () => {
						starts++;
						if (input.scenario === "startup-error") throw new Error("fake startup error");
						if (input.scenario === "startup-abort") controller.abort();
						if (input.scenario === "pollabort" || input.scenario === "retryabort")
							setTimeout(() => controller.abort(), 5);
						if (input.scenario === "complete")
							setTimeout(() => {
								const row = db.claimNextSupervisorRequest(input.db, "fake-owner");
								db.completeSupervisorRequest(input.db, row.id, "fake-owner", {
									kind: "approve",
									reason: "fake approval",
								});
							}, 5);
					},
				},
			);
			const rows = [];
			if (existsSync(input.db))
				for (let id = 1; id < 10; id++) {
					const row = db.readSupervisorRequest(input.db, id);
					if (!row) break;
					rows.push({ id: row.id, kind: row.kind, status: row.status, response: row.response ?? null });
				}
			results.push({ response, starts, rows });
		}
		return results;
	}
	if (request.operation === "approvalScenarios") {
		const results = [];
		for (const raw of requireCases(request)) {
			const input = raw as {
				decision: unknown;
				human: boolean;
				escalate: boolean;
				requestError?: string;
				humanError?: string;
			};
			const log: string[] = [];
			try {
				const value = await (api.reviewToolCallWithSupervisor as (...args: unknown[]) => Promise<unknown>)(
					async () => {
						if (input.requestError) throw new Error(input.requestError);
						return input.decision;
					},
					input.human
						? async (reason: string) => {
								log.push(reason);
								if (input.humanError) throw new Error(input.humanError);
								return { block: true, reason: `human: ${reason}` };
							}
						: undefined,
					input.escalate,
				);
				results.push({ value: value ?? null, log });
			} catch (error) {
				results.push({ error: error instanceof Error ? error.message : String(error), log });
			}
		}
		return results;
	}
	if (request.operation === "wakeCycle") {
		const results = [];
		for (const value of requireCases(request)) {
			const Server = api.SupervisorRequestWakeServer as new (
				path: string,
			) => {
				start(): Promise<void>;
				close(): Promise<void>;
				currentGeneration(): number;
				waitForWakeAfter(generation: number, signal: AbortSignal): Promise<void>;
			};
			const server = new Server(value as string);
			const generations = [];
			for (let cycle = 0; cycle < 2; cycle++) {
				await server.start();
				const observed = server.currentGeneration();
				const waiter = server.waitForWakeAfter(observed, new AbortController().signal);
				setTimeout(() => {
					(api.notifySupervisorRequest as (path: string) => void)(value as string);
				}, 20);
				await waiter;
				generations.push(server.currentGeneration());
				await server.close();
			}
			results.push(generations);
		}
		return results;
	}
	if (request.operation === "vetoScenario") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const db = await import(pathToFileURL(join(dirname(process.argv[2]), "..", "core", "session-control-db.ts")).href);
		const results = [];
		for (const raw of requireCases(request)) {
			const input = raw as { db: string; decision: string; instructions: string; kind: string };
			const id = db.postSupervisorRequest(input.db, {
				kind: input.kind,
				payload: {},
				projectId: "pi",
				senderSessionId: "main",
				deadlineAt: "2099-01-01T00:00:00.000Z",
			});
			const request = db.claimNextSupervisorRequest(input.db, "owner");
			const entries: unknown[] = [];
			const feedback: unknown[] = [];
			const reviewed: string[] = [];
			const session = {
				abort: async () => {},
				prompt: async () => {
					entries.push({
						type: "message",
						id: "response",
						message: {
							role: "assistant",
							content: [
								{
									type: "toolCall",
									id: "call",
									name: "supervisor_response",
									arguments: { kind: "continue", instructions: input.instructions, reason: "proof" },
								},
							],
							stopReason: "toolUse",
						},
					});
				},
				sessionManager: { getBranch: () => entries, getLeafId: () => null },
				sendCustomMessage: async (message: unknown) => {
					feedback.push(message);
				},
			};
			await (api.processSupervisorRequest as (...args: unknown[]) => Promise<void>)(
				input.db,
				request,
				session,
				async (instructions: string) => {
					reviewed.push(instructions);
					if (input.decision === "throw") throw new Error("fake failure");
					return input.decision === "invalid" ? undefined : input.decision;
				},
			);
			results.push({ response: db.readSupervisorRequest(input.db, id).response, feedback, reviewed });
		}
		return results;
	}
	if (request.operation === "startupScenario") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const { VERSION } = await import(pathToFileURL(join(dirname(process.argv[2]), "..", "config.ts")).href);
		const results = [];
		for (const raw of requireCases(request)) {
			const input = raw as { scenario: string; db: string };
			const trace: string[] = [];
			const ready = {
				version: VERSION,
				pid: 123,
				executable: "/fake/pi",
				instanceId: "new",
				managedBy: "pi",
				ready: true,
			};
			let resident: unknown =
				input.scenario === "ready"
					? ready
					: input.scenario === "external" || input.scenario === "replace"
						? {
								...ready,
								version: "old",
								instanceId: "old",
								managedBy: input.scenario === "external" ? "external" : "pi",
							}
						: undefined;
			try {
				const result = await (api.ensureSupervisorRunning as (...args: unknown[]) => Promise<unknown>)(
					{
						controlDbPath: input.db,
						pollIntervalMs: 1,
						startupTimeoutMs: input.scenario === "timeout" ? 0 : 100,
						launchInvocation: { command: "/fake/pi", args: ["supervisor"], cwd: "/fake", env: {} },
					},
					{
						probe: async () => {
							trace.push("probe");
							return resident;
						},
						launch: async () => {
							trace.push("launch");
							if (input.scenario === "launch-error") throw new Error("fake launch error");
							resident = ready;
						},
						terminate: async () => {
							trace.push("terminate");
							resident = undefined;
						},
					},
				);
				results.push({ result, trace });
			} catch (error) {
				results.push({ error: error instanceof Error ? error.message : String(error), trace });
			}
		}
		return results;
	}
	if (request.operation === "openResident") {
		const { readdirSync } = await import("node:fs");
		const { basename, join } = await import("node:path");
		return requireCases(request).map((value) => {
			const input = value as { agentDir: string; cwd: string; db: string; role: string };
			const invoke = api[input.role === "supervisor" ? "openSupervisorSession" : "openArchitectSession"] as (
				...args: string[]
			) => { getSessionFile(): string };
			const manager = invoke(input.agentDir, input.cwd, input.db);
			return {
				file: basename(manager.getSessionFile()),
				files: readdirSync(join(input.agentDir, `${input.role}-sessions`)).sort(),
			};
		});
	}
	if (request.operation === "restartPending") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const { SessionManager } = await import(pathToFileURL(join(dirname(process.argv[2]), "session-manager.ts")).href);
		return requireCases(request).map((messages) => {
			const manager = SessionManager.inMemory("/fixture");
			for (const message of messages as unknown[]) manager.appendMessage(message);
			(api.resolveRestartInterruptedToolCalls as (manager: unknown) => void)(manager);
			return manager
				.buildSessionContext()
				.messages.filter((message: { role: string }) => message.role === "toolResult")
				.map((message: Record<string, unknown>) => ({ ...message, timestamp: 0 }));
		});
	}
	const invoke = api[request.operation] as (...args: unknown[]) => unknown;
	return Promise.all(
		requireCases(request).map(async (value) => {
			const args = value as unknown[];
			return (await invoke(...args)) ?? null;
		}),
	);
}).catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
