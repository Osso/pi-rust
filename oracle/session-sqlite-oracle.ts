// Differential oracle for packages/session-backends/sqlite-node.
//
// Protocol: one JSON request per line on stdin, one JSON response per line on stdout.
// argv[2] is the pinned source module path (`.../sqlite-node/src/index.ts`).

import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const FIXED_NOW = 1_700_000_000_000;
Date.now = () => FIXED_NOW;

type OracleRequest = {
	requestId: string | number | null;
	op: string;
	[key: string]: any;
};

type OracleResponse =
	| { requestId: string | number | null; ok: true; output: unknown }
	| {
			requestId: string | number | null;
			ok: false;
			error: { name: string; code: string | null; message: string };
	  };

let api: any;
let envApi: any;
let repository: any;
let search: any;
let root = "";
const sessions = new Map<string, any>();
const metadataById = new Map<string, any>();

function normalizeError(error: unknown): { name: string; code: string | null; message: string } {
	if (error instanceof Error) {
		const code = "code" in error ? String((error as any).code) : null;
		return { name: error.name, code, message: error.message };
	}
	return { name: "UnknownError", code: null, message: String(error) };
}

function session(id: string): any {
	const found = sessions.get(id);
	if (!found) throw new Error(`Unknown session ${id}`);
	return found;
}

function metadata(id: string): any {
	const found = metadataById.get(id);
	if (!found) throw new Error(`Unknown metadata ${id}`);
	return found;
}

function userMessage(text: string): any {
	return { role: "user", content: [{ type: "text", text }], timestamp: 1 };
}

function assistantMessage(text: string): any {
	return {
		role: "assistant",
		content: [{ type: "text", text }],
		api: "anthropic-messages",
		provider: "anthropic",
		model: "claude-sonnet-4-5",
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "stop",
		timestamp: 1,
	};
}

function entryQuery(query: any = {}): any {
	return {
		...(query.order === undefined ? {} : { order: query.order }),
		...(query.limit === undefined ? {} : { limit: query.limit }),
		...(query.type === undefined ? {} : { type: query.type }),
		...(query.customType === undefined ? {} : { customType: query.customType }),
		...(query.cursor === undefined ? {} : { cursor: { afterSeq: query.cursor } }),
	};
}

function branchQuery(query: any = {}): any {
	return {
		...entryQuery(query),
		...(query.start === undefined ? {} : { start: query.start }),
		...(query.stopAtType === undefined ? {} : { stopAtType: query.stopAtType }),
		...(query.stopAtId === undefined ? {} : { stopAtId: query.stopAtId }),
	};
}

function recordQuery(query: any = {}): any {
	return {
		...(query.lane === undefined ? {} : { lane: query.lane }),
		...(query.type === undefined ? {} : { type: query.type }),
		...(query.runId === undefined ? {} : { runId: query.runId }),
		...(query.operationKind === undefined ? {} : { operationKind: query.operationKind }),
		...(query.afterSeq === undefined ? {} : { afterSeq: query.afterSeq }),
		...(query.order === undefined ? {} : { order: query.order }),
		...(query.limit === undefined ? {} : { limit: query.limit }),
	};
}

async function execute(request: OracleRequest): Promise<unknown> {
	const { op } = request;
	switch (op) {
		case "init": {
			root = request.root;
			repository = new api.SqliteSessionRepository({
				env: new envApi.NodeExecutionEnv({ cwd: root }),
				sqlite: api.createNodeSqliteFactory(),
				databasePath: request.databasePath,
				...(request.writerLease === undefined ? {} : { writerLease: request.writerLease }),
			});
			search = api.createSqliteSessionSearch({
				env: new envApi.NodeExecutionEnv({ cwd: root }),
				sqlite: api.createNodeSqliteFactory(),
				databasePath: request.databasePath,
			});
			return null;
		}
		case "create": {
			const created = await repository.create({
				id: request.id,
				cwd: root,
				...(request.parentSessionId === undefined ? {} : { parentSessionId: request.parentSessionId }),
				...(request.metadata === undefined ? {} : { metadata: request.metadata }),
			});
			const createdMetadata = await created.getMetadata();
			sessions.set(request.id, created);
			metadataById.set(request.id, createdMetadata);
			return createdMetadata;
		}
		case "open_by_id": {
			const listed = await repository.list({});
			const found = listed.find((candidate: any) => candidate.id === request.session);
			if (!found) throw new Error(`Session not found: ${request.session}`);
			const opened = await repository.open(found);
			const openedMetadata = await opened.getMetadata();
			sessions.set(request.session, opened);
			metadataById.set(request.session, openedMetadata);
			return openedMetadata;
		}
		case "open": {
			const opened = await repository.open(metadata(request.session));
			sessions.set(request.session, opened);
			return await opened.getMetadata();
		}
		case "append_message": {
			const target = session(request.session);
			const message = request.role === "assistant" ? assistantMessage(request.text) : userMessage(request.text);
			return await target.appendMessage(message);
		}
		case "append_entry":
			return await session(request.session).appendEntry(request.entry, request.lane ?? "main");
		case "append_record":
			return await session(request.session).appendRecord(request.record);
		case "create_lane":
			await session(request.session).createLane(request.lane, request.at ?? null);
			return null;
		case "move_lane":
			await session(request.session).moveLane(request.lane ?? "main", request.to ?? null);
			return null;
		case "set_name":
			await session(request.session).setName(request.name ?? undefined);
			return null;
		case "set_label":
			await session(request.session).setLabel(request.entryId, request.label ?? undefined);
			return null;
		case "get_metadata":
			return await session(request.session).getMetadata();
		case "get_lanes":
			return await session(request.session).getLanes();
		case "get_name":
			return (await session(request.session).getName()) ?? null;
		case "get_label":
			return (await session(request.session).getLabel(request.entryId)) ?? null;
		case "get_stats":
			return await session(request.session).getStats();
		case "get_entry": {
			const found = await session(request.session).getEntry(request.entryId);
			return found === undefined ? null : found;
		}
		case "find_entries":
			return await session(request.session).findEntries(entryQuery(request.query));
		case "find_entries_on_branch":
			return await session(request.session).findEntriesOnBranch(branchQuery(request.query));
		case "find_records":
			return await session(request.session).findRecords(recordQuery(request.query));
		case "find_open_operations":
			return await session(request.session).findOpenOperations(
				request.lane ?? "main",
				request.limit === undefined ? undefined : { limit: request.limit },
			);
		case "get_log":
			return await session(request.session).getLog({
				...(request.afterSeq === undefined ? {} : { afterSeq: request.afterSeq }),
				...(request.limit === undefined ? {} : { limit: request.limit }),
			});
		case "list":
			return await repository.list(
				request.cwd === undefined || request.cwd === false
					? {}
					: { cwd: request.cwd === true ? root : request.cwd },
			);
		case "fork": {
			const forked = await repository.fork(metadata(request.session), {
				...(request.options ?? {}),
				id: request.id,
				cwd: root,
			});
			const forkedMetadata = await forked.getMetadata();
			sessions.set(request.id, forked);
			metadataById.set(request.id, forkedMetadata);
			return forkedMetadata;
		}
		case "repair_branch_cache":
			await repository.repairBranchCache(metadata(request.session));
			return null;
		case "delete":
			await repository.delete(metadata(request.session));
			return null;
		case "search": {
			const hits: unknown[] = [];
			for await (const hit of search.search(request.text, request.options ?? {})) {
				hits.push({
					sessionId: hit.sessionId,
					entryId: hit.entryId,
					timestamp: hit.timestamp,
					score: hit.score,
				});
			}
			return hits;
		}
		case "close":
			await repository.close();
			return null;
		default:
			throw new Error(`Unknown op ${op}`);
	}
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (!sourceModulePath) throw new Error("oracle requires the source module path");
	const moduleDir = dirname(sourceModulePath);
	api = await import(pathToFileURL(sourceModulePath).href);
	envApi = await import(pathToFileURL(join(moduleDir, "../../../agent/src/harness/env/nodejs.ts")).href);

	const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of lines) {
		if (line.trim() === "") continue;
		let request: OracleRequest;
		try {
			request = JSON.parse(line) as OracleRequest;
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ requestId: null, ok: false, error: normalizeError(error) })}\n`);
			continue;
		}
		let response: OracleResponse;
		try {
			const output = await execute(request);
			response = { requestId: request.requestId ?? null, ok: true, output: output === undefined ? null : output };
		} catch (error) {
			response = { requestId: request.requestId ?? null, ok: false, error: normalizeError(error) };
		}
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

void main();
