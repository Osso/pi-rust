import { readFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const supervisorMainPath = process.argv[2];
const packagesRoot = resolve(dirname(supervisorMainPath), "../../..");

type Request = { id: string | number; operation: string; dir: string };

async function loadModules() {
	const main = await import(supervisorMainPath);
	const { SessionManager } = await import(`${packagesRoot}/coding-agent/src/core/session-manager.ts`);
	const controlDb = await import(`${packagesRoot}/coding-agent/src/core/session-control-db.ts`);
	const sqlite = await import(`${packagesRoot}/coding-agent/src/core/sqlite.ts`);
	return { main, SessionManager, controlDb, sqlite };
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
				const object = node as Record<string, unknown>;
				const result: Record<string, unknown> = {};
				for (const [key, child] of Object.entries(object)) {
					if (key === "id" || key === "parentId") result[key] = "<id>";
					else if (key === "timestamp") result[key] = "<timestamp>";
					else if (key === "cwd") result[key] = "<cwd>";
					else result[key] = normalize(child);
				}
				return result;
			};
			return normalize(value);
		});
}

function dumpControlDb(controlDbPath: string, sqlite: { createSqliteDatabase: (path: string) => {
	prepare: (sql: string) => { all: () => unknown[] };
	close: () => void;
} }) {
	const db = sqlite.createSqliteDatabase(controlDbPath);
	try {
		return db
			.prepare("SELECT id, kind, status, response_json FROM supervisor_requests ORDER BY id")
			.all() as unknown[];
	} finally {
		db.close();
	}
}

async function run(request: Request) {
	const { main, SessionManager, controlDb, sqlite } = await loadModules();
	const dir = request.dir;
	mkdirSync(dir, { recursive: true });
	const controlDbPath = join(dir, "control.sqlite");
	const sessionDir = join(dir, "sessions");
	const sessionManager = SessionManager.create(dir, sessionDir, { id: "supervisor" });
	const session = {
		abort: async () => {},
		prompt: async () => {
			sessionManager.appendMessage({
				role: "assistant",
				content: [{ type: "text", text: '{"kind":"approve","reason":"ok"}' }],
				api: "faux",
				provider: "faux",
				model: "faux",
				usage: {
					input: 0,
					output: 0,
					cacheRead: 0,
					cacheWrite: 0,
					totalTokens: 0,
					cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
				},
				stopReason: "stop",
				timestamp: 0,
			});
		},
		sessionId: "supervisor",
		sessionManager,
	};
	controlDb.postSupervisorRequest(controlDbPath, {
		deadlineAt: new Date(Date.now() + 60_000).toISOString(),
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
		processRequest: async (dbPath: string, supervisorRequest: unknown, activeSession: unknown) => {
			await main.processSupervisorRequest(dbPath, supervisorRequest, activeSession);
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
		controlRows: dumpControlDb(controlDbPath, sqlite as unknown as { createSqliteDatabase: (path: string) => { prepare: (sql: string) => { all: () => unknown[] }; close: () => void } }),
		session: sessionFile ? normalizeSessionFile(readFileSync(sessionFile, "utf8")) : [],
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
				process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
			})
			.catch((error: unknown) => {
				const message = error instanceof Error ? error.message : String(error);
				process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, error: { message } })}\n`);
			});
	}
});
