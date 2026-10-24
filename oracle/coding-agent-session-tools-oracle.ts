// Differential oracle for coding-agent session directory and session tools.
// argv[2] is the pinned `packages/coding-agent/src` directory.
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = {
	id: number;
	operation: string;
	[property: string]: unknown;
};

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error
		? { name: error.name, message: error.message }
		: { name: "Unknown", message: String(error) };
}

async function load(src: string, relativePath: string): Promise<any> {
	return import(pathToFileURL(join(src, relativePath)).href);
}

function setupDatabase(controlDb: any, sqlite: any, controlDbPath: string, setup: any[]): void {
	for (const step of setup) {
		switch (step.op) {
			case "metadata":
				controlDb.writeSessionMetadata(controlDbPath, {
					sessionPath: step.sessionPath,
					id: step.id,
					cwd: step.cwd,
					name: step.name,
					parentSessionPath: step.parentSessionPath,
					isSubagent: step.isSubagent,
					createdAt: step.createdAt ?? "2026-01-01T00:00:00.000Z",
					modifiedAt: step.modifiedAt ?? "2026-01-01T00:10:00.000Z",
					messageCount: step.messageCount ?? 1,
					firstMessage: step.firstMessage ?? "hello",
					allMessagesText: step.allMessagesText ?? "hello",
				});
				break;
			case "goal":
				controlDb.writeSessionGoal(controlDbPath, step.sessionPath, step.goalJson);
				break;
			case "health":
				controlDb.writeSessionHealth(controlDbPath, step.record);
				break;
			case "listener":
				controlDb.registerRuntimeMailboxListener(
					controlDbPath,
					{ agentId: step.agentId ?? null, sessionId: step.sessionId },
					step.pid === "self" ? process.pid : step.pid,
					step.sessionPath,
				);
				break;
			case "archive":
				controlDb.archiveSession(controlDbPath, step.sessionPath);
				break;
			default:
				throw new Error(`unknown setup op ${step.op}`);
		}
	}
	if (setup.some((step) => step.op === "listener")) {
		const listenerTimestamps: Record<string, string> = {};
		for (const step of setup) {
			if (step.op === "listener") listenerTimestamps[step.sessionId] = step.updatedAt ?? "2026-01-01T00:10:00.000Z";
		}
		const db = sqlite.createSqliteDatabase(controlDbPath);
		for (const [sessionId, updatedAt] of Object.entries(listenerTimestamps)) {
			db.prepare("UPDATE runtime_mailbox_listeners SET updated_at = ? WHERE recipient_session_id = ?").run(
				updatedAt,
				sessionId,
			);
		}
		db.close();
	}
}

function stableSessions(sessions: any[]): unknown[] {
	return sessions.map((session) => ({
		sessionId: session.sessionId,
		sessionPath: session.sessionPath,
		pid: session.pid,
		status: session.status,
		startedAt: session.startedAt,
		lastActiveAt: session.lastActiveAt,
		name: session.name,
		goal: session.goal,
		cwd: session.cwd,
		lastCheckedAt: session.lastCheckedAt,
		checkStatus: session.checkStatus,
		checkLatencyMs: session.checkLatencyMs,
		agentGeneration: session.agentGeneration,
		checkedGeneration: session.checkedGeneration,
		eligibleToReceive: session.eligibleToReceive,
	}));
}

function directoryOptions(request: Request): unknown {
	return {
		now: () => new Date(request.nowIso as string),
		includeEnded: request.includeEnded,
		isRuntimeProcessAlive: (pid: number) =>
			(request.alivePids as Array<number | string>).some((value) => (value === "self" ? pid === process.pid : value === pid)),
	};
}

async function toolContext(src: string, request: Request): Promise<any> {
	const sessionManagerModule = await load(src, "core/session-manager.ts");
	const sessionManager = request.sessionFile
		? sessionManagerModule.SessionManager.open(
				request.sessionFile as string,
				request.sessionDir as string | undefined,
				request.cwd as string | undefined,
			)
		: {
				getSessionId: () => request.sessionId ?? "unknown-session",
				getSessionFile: () => undefined,
			};
	const context: Record<string, unknown> = {
		controlDbPath: request.controlDbPath,
		cwd: request.cwd,
		mode: "print",
		hasUI: false,
		multiAgentAgentId: request.multiAgentAgentId,
		multiAgentRequiresAgentId: request.multiAgentRequiresAgentId,
		noSupervisor: request.noSupervisor,
		sessionManager,
	};
	if (request.uiScript) {
		const selects = [...((request.uiScript as any).selects ?? [])];
		const inputs = [...((request.uiScript as any).inputs ?? [])];
		context.hasUI = true;
		context.mode = "tui";
		context.ui = {
            custom: async () => (request.uiScript as any).panelResult,
			select: async () => selects.shift(),
			input: async () => inputs.shift(),
		};
	}
	if (request.toolMode === "resume") {
		const switched: Record<string, unknown> = { path: undefined, starter: undefined };
		context.switchSession = async (sessionPath: string, options?: any) => {
			switched.path = sessionPath;
			if (request.captureSwitchOptions) {
				switched.hasOptions = options !== undefined;
				switched.hasWithSession = typeof options?.withSession === "function";
			}
			if (request.switchError !== undefined) {
				throw new Error(request.switchError as string);
			}
			const sendStarter = async (message: string) => {
				switched.starter = message;
				if (request.starterError !== undefined) {
					throw new Error(request.starterError as string);
				}
			};
			if (options?.withSession) {
				await options.withSession({ sendUserMessage: sendStarter });
			}
			return { cancelled: request.cancelled === true };
		};
		(request as any).switched = switched;
	}
	if (request.toolMode === "change-cwd") {
		const relocations: unknown[] = [];
		context.relocateAfterToolResult = (toolCallId: string, targetCwd: string) =>
			relocations.push({ toolCallId, targetCwd });
		(request as any).relocations = relocations;
	}
	return context;
}

const handlers: Record<string, (src: string, request: Request) => Promise<unknown>> = {
	"session-directory:list": async (src, request) => {
		const directory = await load(src, "core/session-directory.ts");
		const controlDb = await load(src, "core/session-control-db.ts");
		const sqlite = await load(src, "core/sqlite.ts");
		setupDatabase(controlDb, sqlite, request.controlDbPath as string, request.setup as any[]);
		const sessions = directory.listSessions(request.controlDbPath as string, directoryOptions(request));
		return { sessions: stableSessions(sessions) };
	},
	"session-directory:broadcast": async (src, request) => {
		const directory = await load(src, "core/session-directory.ts");
		const controlDb = await load(src, "core/session-control-db.ts");
		const sqlite = await load(src, "core/sqlite.ts");
		const controlDbPath = request.controlDbPath as string;
		setupDatabase(controlDb, sqlite, controlDbPath, request.setup as any[]);
		const input = request.broadcast as any;
		const results = directory.broadcastToSessions(controlDbPath, input, directoryOptions(request));
		return {
			results,
			listeners: controlDb.listRuntimeMailboxListeners(controlDbPath),
			health: controlDb.listSessionHealth(controlDbPath),
			messages: controlDb.listRuntimeMailboxMessages(controlDbPath).map((message: any) => ({
				recipient: message.recipient,
				sender: message.sender,
				kind: message.kind,
				body: message.body,
			})),
		};
	},
    "history:definition": async (src) => {
        const module = await load(src, "core/tools/search-current-session-history.ts");
        const tool = module.createSearchCurrentSessionHistoryToolDefinition();
        return {
            name: tool.name,
            label: tool.label,
            description: tool.description,
            promptSnippet: tool.promptSnippet,
            promptGuidelines: tool.promptGuidelines,
            parameters: tool.parameters,
            executionMode: tool.executionMode,
        };
    },
	"tool:execute": async (src, request) => {
		if (request.setup) {
			const controlDb = await load(src, "core/session-control-db.ts");
			const sqlite = await load(src, "core/sqlite.ts");
			setupDatabase(controlDb, sqlite, request.controlDbPath as string, request.setup as any[]);
		}
		const module = await load(src, request.module as string);
		const factory = module[request.factory as string];
		const tool = factory();
		const context = await toolContext(src, request);
		try {
			const result = await tool.execute("tool-call", request.params, undefined, undefined, context);
			const response: Record<string, unknown> = {
				ok: true,
				content: result.content,
				details: result.details ?? null,
				isError: result.isError ?? null,
				terminate: result.terminate ?? null,
			};
			if (request.toolMode === "resume") response.switched = (request as any).switched;
			if (request.toolMode === "change-cwd") response.relocations = (request as any).relocations;
			return response;
		} catch (error) {
			const response: Record<string, unknown> = {
				ok: false,
				error: errorOf(error).message,
			};
			if (request.captureSwitchFailure) response.switched = request.switched;
			return response;
		}
	},
	"tool:channel-post-state": async (src, request) => {
		const controlDb = await load(src, "core/session-control-db.ts");
		const controlDbPath = request.controlDbPath as string;
		const messages = controlDb.listSharedChannelMessagesAfter(controlDbPath, 0, 100, Number.MAX_SAFE_INTEGER);
		const cursor = controlDb.readSharedChannelCursor(controlDbPath, {
			agentId: request.agentId ?? null,
			sessionId: request.sessionId,
		});
		return {
			messages: messages.map((message: any) => ({
				id: message.id,
				body: message.body,
				sender: message.sender,
			})),
			cursor: cursor ?? null,
		};
	},
	"tool:architect-state": async (src, request) => {
		const controlDb = await load(src, "core/session-control-db.ts");
		const requests = controlDb.listPendingArchitectRequests(request.controlDbPath as string, 10);
		return requests.map((entry: any) => ({
			senderSessionId: entry.senderSessionId,
			projectCwd: entry.projectCwd ?? null,
			body: entry.body,
			status: entry.status,
		}));
	},
};

async function main(): Promise<void> {
	const src = process.argv[2];
	if (!src) throw new Error("missing source directory argument");
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		if (!line.trim()) continue;
		const request = JSON.parse(line) as Request;
		try {
			const handler = handlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			const output = await handler(src, request);
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main();
