import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { removeAbandonedEmptySession, sweepAbandonedEmptySessions } =
	await import(pathToFileURL(`${root}/core/empty-session-cleanup.ts`).href);
const {
	writeSessionMetadata,
	readSessionMetadata,
	listSessionMetadata,
	writeSessionHealth,
} = await import(pathToFileURL(`${root}/core/session-control-db.ts`).href);

interface Row {
	id: string;
	sessionPath: string;
	cwd: string;
	messageCount: number;
	archivedAt?: string;
	isSubagent?: boolean;
	existing?: boolean;
	pid?: "self" | "dead" | "zero";
}

function seed(db: string, row: Row) {
	writeSessionMetadata(db, {
		...row,
		createdAt: "2026-10-23T00:00:00.000Z",
		modifiedAt: "2026-10-23T00:00:00.000Z",
		firstMessage: "",
	});
	if (row.existing) {
		mkdirSync(dirname(row.sessionPath), { recursive: true });
		writeFileSync(row.sessionPath, "fixture");
	}
	if (row.pid !== undefined) {
		const pid =
			row.pid === "self" ? process.pid : row.pid === "zero" ? 0 : 2147483647;
		writeSessionHealth(db, {
			sessionId: row.id,
			agentGeneration: 1,
			pid,
			lastActiveAt: null,
			lastCheckedAt: null,
			checkStatus: "ok",
			checkedGeneration: null,
			checkLatencyMs: null,
			updatedAt: "2026-10-23T00:00:00.000Z",
		});
	}
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			operation: "remove" | "sweep";
			db: string | null;
			file: string | null;
			cwd: string;
			id: string;
			entries: Array<{ type: string }>;
			target: string | null;
			existing: boolean;
			rows: Row[];
		};
		if (request.operation === "remove") {
			if (request.db && request.file)
				seed(request.db, {
					id: request.id,
					sessionPath: request.file,
					cwd: request.cwd,
					messageCount: 0,
					existing: request.existing,
				});
			else if (request.file && request.existing)
				writeFileSync(request.file, "fixture");
			const manager = {
				getMetadataControlDbPath: () => request.db ?? undefined,
				getSessionFile: () => request.file ?? undefined,
				getSessionId: () => request.id,
				getEntries: () => request.entries,
			};
			removeAbandonedEmptySession(manager, request.target ?? undefined);
			console.log(
				JSON.stringify({
					ok: true,
					fileExists: Boolean(request.file && existsSync(request.file)),
					metadataExists: Boolean(
						request.db &&
							request.file &&
							readSessionMetadata(request.db, request.file),
					),
				}),
			);
		} else {
			if (!request.db) throw new Error("sweep DB required");
			for (const row of request.rows) seed(request.db, row);
			const count = sweepAbandonedEmptySessions(request.db);
			const remaining = listSessionMetadata(request.db)
				.map((row: { id: string }) => row.id)
				.sort();
			console.log(JSON.stringify({ ok: true, count, remaining }));
		}
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
