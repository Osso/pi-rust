import { mkdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const architectMainPath = process.argv[2];
const packagesRoot = resolve(dirname(architectMainPath), "../../..");

type Request = { id: string | number; operation: string; dir: string };

function normalizeText(node: unknown, dir: string): unknown {
	if (typeof node === "string") {
		return node
			.replaceAll(dir, "<dir>")
			.replaceAll(/\d{4}-\d{2}-\d{2}T[0-9:.]+Z/g, "<timestamp>")
			.replaceAll(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, "<uuid>");
	}
	if (Array.isArray(node)) return node.map((item) => normalizeText(item, dir));
	if (typeof node !== "object" || node === null) return node;
	const result: Record<string, unknown> = {};
	for (const [key, child] of Object.entries(node as Record<string, unknown>)) {
		result[key] = normalizeText(child, dir);
	}
	return result;
}

function normalizeSessionFile(text: string, dir: string): unknown[] {
	return text
		.split("\n")
		.filter((line) => line.length > 0)
		.map((line) => {
			const normalize = (node: unknown): unknown => {
				if (Array.isArray(node)) return node.map(normalize);
				if (typeof node !== "object" || node === null) return node;
				const result: Record<string, unknown> = {};
				for (const [key, child] of Object.entries(node as Record<string, unknown>)) {
					if (key === "id" || key === "parentId") result[key] = "<id>";
					else if (key === "timestamp") result[key] = "<timestamp>";
					else if (key === "cwd") result[key] = "<cwd>";
					else result[key] = normalize(child);
				}
				return result;
			};
			const normalized = normalize(JSON.parse(line));
			return normalizeText(normalized, dir);
		});
}

async function run(request: Request) {
	const main = await import(architectMainPath);
	const { ArchitectObserver } = await import(`${packagesRoot}/coding-agent/src/architect/observer.ts`);
	const controlDb = await import(`${packagesRoot}/coding-agent/src/core/session-control-db.ts`);
	const sqlite = await import(`${packagesRoot}/coding-agent/src/core/sqlite.ts`);
	const { SessionManager } = await import(`${packagesRoot}/coding-agent/src/core/session-manager.ts`);
	const dir = request.dir;
	mkdirSync(dir, { recursive: true });
	const controlDbPath = join(dir, "control.sqlite");
	const sessionDir = join(dir, "sessions");
	const sessionManager = SessionManager.create(dir, sessionDir, { id: "architect" });
	controlDb.postArchitectRequest(controlDbPath, {
		body: "review this design",
		projectCwd: dir,
		senderSessionId: "main-session",
	});
	const observer = new ArchitectObserver(controlDbPath);
	let promptText = "";
	await main.runArchitectCycle(observer, async (content: string) => {
		promptText = content;
		sessionManager.appendMessage({
			role: "assistant",
			content: [{ type: "text", text: content }],
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
	});
	main.completeSentArchitectRequest(
		{ completeRequest: (id: number, sessionId: string) => observer.completeRequest(id, sessionId) },
		{ threadId: "architect-request:1", toAgentId: "main", toSessionId: "main-session" },
	);
	const db = (sqlite as { createSqliteDatabase: (path: string) => {
		prepare: (sql: string) => { all: () => unknown[] };
		close: () => void;
	} }).createSqliteDatabase(controlDbPath);
	let rows: unknown[];
	try {
		rows = db
			.prepare("SELECT id, status, completed_at, sender_session_id FROM architect_requests ORDER BY id")
			.all();
	} finally {
		db.close();
	}
	const normalizedRows = (rows as Array<Record<string, unknown>>).map((row) => ({
		...row,
		completed_at: row.completed_at === null ? null : "<timestamp>",
	}));
	const sessionFile = sessionManager.getSessionFile();
	return {
		controlRows: normalizedRows,
		prompt: promptText
			.replaceAll(dir, "<dir>")
			.replaceAll(/\d{4}-\d{2}-\d{2}T[0-9:.]+Z/g, "<timestamp>")
			.replaceAll(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, "<uuid>"),
		session: sessionFile ? normalizeSessionFile(readFileSync(sessionFile, "utf8"), dir) : [],
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
