import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const {
	applySelfRestartRequest,
	consumeSelfRestartRequest,
	resolveRestartInterruptedToolCalls,
} = await import(pathToFileURL(`${source}/core/self-restart.ts`).href);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
const { readMultiAgentState } = await import(
	pathToFileURL(`${source}/core/session-control-db.ts`).href
);
const { createSqliteDatabase } = await import(
	pathToFileURL(`${source}/core/sqlite.ts`).href
);

interface Request {
	op: "apply" | "consume" | "resolve";
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
		const start = Date.now();
		resolveRestartInterruptedToolCalls(manager);
		const end = Date.now();
		const results = manager
			.buildSessionContext()
			.messages.filter(
				(message: { role: string }) => message.role === "toolResult",
			);
		console.log(
			JSON.stringify({
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
