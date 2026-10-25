// Differential/cross-implementation oracle for the shared control database.
// argv[2] is the pinned `packages/coding-agent/src` directory.
import { selectApprovedModule } from "./approved-delta.mjs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, (...args: unknown[]) => unknown> & Record<string, unknown>;
type Step = { op: string; args?: unknown[]; path?: string; count?: number; prefix?: string };
type Request = { id: number; steps: Step[] };
type FixtureManager = {
	appendMessage(message: unknown, refs?: unknown): string;
	appendCustomEntry(customType: string, data?: unknown): string;
	persistForRecovery(): void;
	getSessionFile(): string | undefined;
};

type StepResult = { ok: true; value: unknown } | { ok: false; error: { name: string; message: string } };

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

function normalize(value: unknown): unknown {
	return value === undefined ? null : value;
}

function exerciseRetainedReentry(api: Api, path: string): unknown {
	const release = api.retainControlDbConnection(path) as () => void;
	const insertAndFail = () => {
		api.enqueueIncomingMessage(path, "rollback");
		throw new Error("nested rollback");
	};
	try {
		const id = api.withControlDb(path, () => api.enqueueIncomingMessage(path, "nested"));
		try {
			api.withControlDb(path, (db: unknown) => api.withImmediateTransaction(db, insertAndFail));
		} catch (error) {
			if (!(error instanceof Error) || error.message !== "nested rollback") throw error;
		}
		return {
			id,
			remaining: api.claimLatestIncomingMessage(path),
			second: normalize(api.claimLatestIncomingMessage(path)),
		};
	} finally {
		release();
	}
}

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("control-db oracle requires the coding-agent src directory");
	const module = selectApprovedModule(`${sourceRoot}/core/session-control-db.ts`, "packages/coding-agent/src/core/session-control-db.ts");
	const api = (await import(pathToFileURL(module).href)) as Api;
	const sqlite = (await import(pathToFileURL(`${sourceRoot}/core/sqlite.ts`).href)) as Api;
	const transcript = (await import(pathToFileURL(`${sourceRoot}/core/runtime-mailbox-transcript.ts`).href)) as Api;
	const { SessionManager } = (await import(pathToFileURL(`${sourceRoot}/core/session-manager.ts`).href)) as {
		SessionManager: { create(cwd: string, sessionDir: string): FixtureManager };
	};

	const execute = (step: Step): unknown => {
		if (step.op === "retainedNested") {
			const [path] = step.args as [string];
			return exerciseRetainedReentry(api, path);
		}
		if (step.op === "registerThrow") {
			const [path, recipient] = step.args as [string, unknown];
			let calls = 0;
			const failure = Object.assign(new TypeError("runtime probe failed"), { code: "FIXTURE" });
			try {
				api.registerRuntimeMailboxListener(path, recipient, process.pid, undefined, {
					runtimeInstanceId: "replacement",
					isRuntimeProcessAlive: () => { calls++; throw failure; },
				});
				return { calls, ok: true };
			} catch (error) {
				if (error !== failure) throw error;
				return { calls, ok: false, error: { name: failure.name, message: failure.message, code: failure.code } };
			}
		}
		if (step.op === "takeThrow") {
			const [path, recipient, throwAt] = step.args as [string, unknown, number];
			let calls = 0;
			const failure = Object.assign(new TypeError("eligibility failed"), { code: "FIXTURE" });
			try {
				const delivered = api.takeRuntimeMailboxMessagesForDelivery(path, recipient, () => {
					calls++;
					if (calls === throwAt) throw failure;
					return true;
				});
				return { calls, ok: true, delivered };
			} catch (error) {
				if (error !== failure) throw error;
				return { calls, ok: false, error: { name: failure.name, message: failure.message, code: failure.code } };
			}
		}
		if (step.op === "takeAll") {
			const [path, recipient, limit] = step.args as [string, unknown, number];
			return api.takeRuntimeMailboxMessagesForDelivery(path, recipient, () => true, limit);
		}
		if (step.op === "registerSelf") {
			const [path, recipient, sessionPath] = step.args as [string, unknown, string];
			api.registerRuntimeMailboxListener(path, recipient, process.pid, sessionPath);
			return api.getRuntimeProcessInstanceId();
		}
		if (step.op === "createReceipt") {
			const [directory, wire, customOnly] = step.args as [string, string, boolean?];
			const data = JSON.parse(wire) as { mailboxStoreRefs?: unknown };
			const manager = SessionManager.create("/fixture", directory);
			if (!customOnly) manager.appendMessage({ role: "user", content: "fixture", timestamp: Date.now() }, data.mailboxStoreRefs);
			manager.appendCustomEntry("runtime_mailbox_acceptance", data);
			manager.persistForRecovery();
			return manager.getSessionFile();
		}
		if (step.op === "refsEqualWire") {
			const [left, right] = step.args as [string, string];
			return transcript.mailboxStoreRefsEqual(JSON.parse(left), JSON.parse(right));
		}
		if (step.op === "acceptedRefsWire") return JSON.stringify(transcript.readAcceptedRuntimeMailboxStoreRefs(step.path));
		if (step.op === "acceptedRefs") return transcript.readAcceptedRuntimeMailboxStoreRefs(step.path);
		if (step.op === "clearAgentActivity") {
			const [path, sessionPath, agentId, updatedAt] = step.args as [string, string, string, string];
			return api.updateMultiAgentAgentActivity(path, sessionPath, agentId, undefined, updatedAt);
		}
		if (step.op === "archiveCandidates") {
			const [path, cutoff] = step.args as [string, string];
			return api.archiveSessionsOlderThan(path, new Date(cutoff));
		}
		if (step.op === "callWire") {
			const [fn, wire] = step.args as [string, string];
			const implementation = api[fn];
			if (typeof implementation !== "function") throw new Error(`unknown control-db function ${fn}`);
			return JSON.stringify(normalize(implementation(...JSON.parse(wire))));
		}
		if (step.op === "call") {
			const [fn, args] = step.args as [string, unknown[]];
			const implementation = api[fn];
			if (typeof implementation !== "function") throw new Error(`unknown control-db function ${fn}`);
			return implementation(...(args ?? []));
		}
		// Fixture mutations run inside a schema initialized by the actual public API.
		if (step.op === "fixtureExec") {
			const [path, sql] = step.args as [string, string];
			return api.withControlDb(path, (db: { exec(sql: string): void }) => db.exec(sql));
		}
		if (step.op === "retentionAt") {
			const [path, now] = step.args as [string, number];
			const OriginalDate = globalThis.Date;
			class FixtureDate extends OriginalDate {
				constructor(value?: string | number) {
					if (value === undefined) super(now);
					else super(value);
				}
				static now(): number { return now; }
			}
			globalThis.Date = FixtureDate as DateConstructor;
			try { return api.listRuntimeMailboxMessages(path); }
			finally { globalThis.Date = OriginalDate; }
		}
		if (step.op === "fixtureSql") {
			const [path, sql, parameters = []] = step.args as [string, string, unknown[]];
			return api.withControlDb(path, (db: { prepare(sql: string): { all(...args: unknown[]): unknown[] } }) =>
				db.prepare(sql).all(...parameters),
			);
		}
		if (step.op === "schema") {
			const db = sqlite.createSqliteDatabase(step.path as string) as {
				exec(sql: string): void;
				prepare(sql: string): { all(): unknown[]; get(): unknown };
				close(): void;
			};
			try {
				sqlite.configureSharedSqliteDatabase(db);
				const read = (sql: string) => {
					const row = db.prepare(sql).get();
					if (row === null || row === undefined) return "";
					if (typeof row === "string" || typeof row === "number") return String(row).toLowerCase();
					const value = Object.values(row as Record<string, unknown>)[0];
					return typeof value === "string" || typeof value === "number" ? String(value).toLowerCase() : "";
				};
				const rows = db
					.prepare(
						"SELECT type, name, sql FROM sqlite_master WHERE type IN ('table', 'index', 'trigger') ORDER BY type, name",
					)
					.all();
				return {
					busyTimeout: read("PRAGMA busy_timeout"),
					journalMode: read("PRAGMA journal_mode"),
					rows,
					synchronous: read("PRAGMA synchronous"),
					userVersion: read("PRAGMA user_version"),
				};
			} finally {
				db.close();
			}
		}
		if (step.op === "channelSpam") {
			const [controlDbPath, senderSessionId, count, prefix] = step.args as [string, string, number, string];
			const ids: number[] = [];
			for (let index = 0; index < count; index += 1) {
				ids.push(
					api.postSharedChannelMessage(controlDbPath, {
						body: `${prefix}${index}`,
						sender: { agentId: null, sessionId: senderSessionId },
					}) as number,
				);
			}
			return ids;
		}
		throw new Error(`unknown operation ${step.op}`);
	};

	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const results: StepResult[] = [];
			for (const step of request.steps) {
				try {
					results.push({ ok: true, value: normalize(execute(step)) });
				} catch (error) {
					results.push({ ok: false, error: errorOf(error) });
				}
			}
			process.stdout.write(`${JSON.stringify({ id, ok: true, output: results })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`control-db oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
