import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
const source = await import(
	pathToFileURL(join(root, "terminal-outbox-delivery.ts")).href
);
const sessions = await import(
	pathToFileURL(join(root, "session-manager.ts")).href
);
const stores = await import(
	pathToFileURL(join(root, "multi-agent-store.ts")).href
);
type Request = {
	id: number;
	op: string;
	dir?: string;
	db?: string;
	path?: string;
	now?: number;
	nowKind?: string;
	last?: number;
	lastKind?: string;
	clock?: string;
	noPersistence?: boolean;
	throwAt?: number;
	clockValue?: string;
};
function decodeNumber(
	value: number | undefined,
	kind?: string,
): number | undefined {
	if (kind === "nan") return Number.NaN;
	if (kind === "infinity") return Number.POSITIVE_INFINITY;
	if (kind === "negativeInfinity") return Number.NEGATIVE_INFINITY;
	return value;
}
function projectError(error: unknown): unknown {
	if (!(error instanceof Error)) return error;
	return {
		name: error.name,
		message: error.message,
		...Object.fromEntries(
			Object.entries(error).filter(([key]) => key !== "stack"),
		),
	};
}
function handle(request: Request): unknown {
	if (request.op === "create") {
		const session = sessions.SessionManager.create(request.dir, request.dir);
		session.setMetadataControlDbPath(request.db);
		session.persistForRecovery();
		return { path: session.getSessionFile() };
	}
	if (request.op === "helpers") {
		const now = decodeNumber(request.now, request.nowKind);
		let threshold: unknown;
		try {
			threshold = {
				ok: true,
				value: source.terminalOutboxRetentionThreshold(now),
			};
		} catch (error) {
			threshold = { ok: false, error: projectError(error) };
		}
		return {
			due: source.isTerminalOutboxCleanupDue(
				decodeNumber(request.last, request.lastKind),
				now,
			),
			threshold,
		};
	}
	const store = new stores.MultiAgentStore();
	if (!request.noPersistence) {
		const session = sessions.SessionManager.open(request.path);
		session.setMetadataControlDbPath(request.db);
		store.setPersistenceSessionManager(session);
	}
	let calls = 0;
	const now = (): string => {
		calls++;
		const shouldThrow =
			request.throwAt === undefined || calls === request.throwAt;
		if (shouldThrow && request.clock === "throw-error")
			throw Object.assign(new TypeError("clock failed"), { code: "FIXTURE" });
		if (shouldThrow && request.clock === "throw-null") throw null;
		return request.clockValue ?? request.clock ?? "not a date";
	};
	try {
		const delivered = source.deliverTerminalOutboxProjections({
			controlDbPath: request.db,
			claimId: "fixture",
			artifactRoot: request.dir,
			store,
			now,
		});
		const result = { ok: true, delivered, calls };
		return request.op === "project"
			? { ...result, agents: store.listAgents() }
			: result;
	} catch (error) {
		const result = { ok: false, error: projectError(error), calls };
		return request.op === "project"
			? { ...result, agents: store.listAgents() }
			: result;
	}
}
for await (const line of createInterface({ input: process.stdin })) {
	const request: Request = JSON.parse(line);
	console.log(
		JSON.stringify({ id: request.id, ok: true, output: handle(request) }),
	);
}
