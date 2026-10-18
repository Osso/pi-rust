import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type OracleRequest = {
	id: RequestId;
	op: string;
	[key: string]: any;
};

type OracleResponse =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; code: string | null; message: string } };

let sessionApi: any;
let repoApi: any;
let envApi: any;
let searchApi: any;
let codecApi: any;

let fixedNow = 1_700_000_000_000;
Date.now = () => fixedNow;

let root = "";
let mode: "jsonl" | "memory" = "jsonl";
let env: any;
let repository: any;
const sessions = new Map<string, any>();
const metadataByKey = new Map<string, any>();

function normalizeError(error: unknown): { name: string; code: string | null; message: string } {
	if (error instanceof Error) {
		const code = "code" in error ? String((error as any).code) : null;
		return { name: error.name, code, message: error.message };
	}
	return { name: "UnknownError", code: null, message: String(error) };
}

function normalizePath(path: string): string {
	const withRoot = path.startsWith(root) ? `<root>${path.slice(root.length)}` : path;
	return withRoot
		.split("/")
		.map((part) => (part.startsWith("--") && part.endsWith("--") ? "<sessionDir>" : part))
		.join("/");
}

function normalizeMetadata(metadata: any): any {
	if (!metadata) return metadata;
	const { modifiedAt, ...rest } = metadata;
	const normalized: any = { ...rest };
	if (typeof rest.cwd === "string") normalized.cwd = normalizePath(rest.cwd);
	if (typeof rest.path === "string") normalized.path = normalizePath(rest.path);
	if (typeof modifiedAt === "number") normalized.modified = modifiedAt > 0;
	return normalized;
}

function normalizeEntry(entry: any): any {
	return entry;
}

function normalizeRecord(record: any): any {
	return record;
}

function normalizeLogItem(item: any): any {
	return item;
}

function normalizeStats(stats: any): any {
	return stats;
}

function normalizeQuery(query: any): any {
	if (!query) return {};
	const normalized: any = { ...query };
	if (normalized.cursor) normalized.cursor = { afterSeq: normalized.cursor.afterSeq };
	return normalized;
}

function requireSession(key: string): any {
	const session = sessions.get(key);
	if (!session) throw new Error(`Unknown session key: ${key}`);
	return session;
}

function queryValue(value: any): any {
	return value === undefined ? undefined : value;
}

function appendData(request: any): any {
	return Object.prototype.hasOwnProperty.call(request, "data") ? request.data : undefined;
}

function listFiles(directory: string, prefix = ""): Record<string, string> {
	const files: Record<string, string> = {};
	for (const entry of readdirSync(directory, { withFileTypes: true })) {
		const path = join(directory, entry.name);
		const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
		if (entry.isDirectory()) Object.assign(files, listFiles(path, relative));
		else files[relative] = readFileSync(path, "utf8");
	}
	return files;
}

async function execute(request: OracleRequest): Promise<unknown> {
	switch (request.op) {
		case "reset": {
			root = request.root;
			mode = request.mode ?? "jsonl";
			fixedNow = request.clock ?? 1_700_000_000_000;
			sessions.clear();
			metadataByKey.clear();
			env = new envApi.NodeExecutionEnv({ cwd: root });
			repository =
				mode === "jsonl"
					? new sessionApi.JsonlSessionRepo({ fs: env, sessionsRoot: root })
					: new sessionApi.InMemorySessionRepo();
			return { root };
		}
		case "create": {
			const options: any = { id: request.sessionId, cwd: request.cwd };
			if (request.parentSessionId !== undefined) options.parentSessionId = request.parentSessionId;
			if (request.metadata !== undefined) options.metadata = request.metadata;
			const session =
				mode === "jsonl" ? await repository.create(options) : await repository.create({ id: request.sessionId, parentSessionId: request.parentSessionId });
			sessions.set(request.key, session);
			metadataByKey.set(request.key, await session.getMetadata());
			return normalizeMetadata(await session.getMetadata());
		}
		case "open": {
			const sourceMetadata = metadataByKey.get(request.sourceKey);
			if (!sourceMetadata) throw new Error(`Unknown metadata key: ${request.sourceKey}`);
			const session = await repository.open(sourceMetadata);
			sessions.set(request.key, session);
			metadataByKey.set(request.key, await session.getMetadata());
			return normalizeMetadata(await session.getMetadata());
		}
		case "fork": {
			const sourceMetadata = metadataByKey.get(request.sourceKey);
			if (!sourceMetadata) throw new Error(`Unknown metadata key: ${request.sourceKey}`);
			const options: any = { id: request.sessionId };
			if (mode === "jsonl") options.cwd = request.cwd;
			if (request.parentSessionId !== undefined) options.parentSessionId = request.parentSessionId;
			const session = await repository.fork(sourceMetadata, { ...(request.options ?? {}), ...options });
			sessions.set(request.key, session);
			metadataByKey.set(request.key, await session.getMetadata());
			return normalizeMetadata(await session.getMetadata());
		}
		case "list": {
			const listed = await repository.list(mode === "jsonl" ? { cwd: request.cwd } : undefined);
			return listed.map(normalizeMetadata);
		}
		case "delete": {
			await repository.delete(metadataByKey.get(request.key));
			return null;
		}
		case "append_entry": {
			const entry = await requireSession(request.key).appendEntry(request.entry, request.lane ?? "main");
			return normalizeEntry(entry);
		}
		case "append_message": {
			const id = await requireSession(request.key).appendMessage(request.message);
			return id;
		}
		case "append_compaction": {
			const id = await requireSession(request.key).appendCompaction(
				request.summary,
				request.tokensBefore,
				request.retainedTail ?? [],
				request.options,
			);
			return id;
		}
		case "append_custom": {
			const id = await requireSession(request.key).appendCustomEntry(request.customType, appendData(request));
			return id;
		}
		case "append_record": {
			return normalizeRecord(await requireSession(request.key).appendRecord(request.record));
		}
		case "create_lane": {
			await requireSession(request.key).createLane(request.lane, queryValue(request.at) ?? null);
			return null;
		}
		case "move_lane": {
			await requireSession(request.key).moveLane(request.lane, queryValue(request.to) ?? null);
			return null;
		}
		case "set_name": {
			await requireSession(request.key).setName(queryValue(request.name) ?? undefined);
			return null;
		}
		case "set_label": {
			await requireSession(request.key).setLabel(request.entryId, queryValue(request.label) ?? undefined);
			return null;
		}
		case "find_entries": {
			return (await requireSession(request.key).findEntries(normalizeQuery(request.query))).map(normalizeEntry);
		}
		case "find_entry": {
			return normalizeEntry(await requireSession(request.key).findEntry(normalizeQuery(request.query)));
		}
		case "find_entries_on_branch": {
			return (await requireSession(request.key).findEntriesOnBranch(normalizeQuery(request.query))).map(normalizeEntry);
		}
		case "find_entry_on_branch": {
			return normalizeEntry(await requireSession(request.key).findEntryOnBranch(normalizeQuery(request.query)));
		}
		case "find_records": {
			return (await requireSession(request.key).findRecords(normalizeQuery(request.query))).map(normalizeRecord);
		}
		case "find_open_operations": {
			return (await requireSession(request.key).findOpenOperations(request.lane, request.limit === undefined ? undefined : { limit: request.limit })).map(normalizeRecord);
		}
		case "get_log": {
			return (await requireSession(request.key).getLog(request.options ?? {})).map(normalizeLogItem);
		}
		case "get_lanes": {
			return await requireSession(request.key).getLanes();
		}
		case "get_stats": {
			return normalizeStats(await requireSession(request.key).getStats());
		}
		case "get_name": {
			return (await requireSession(request.key).getName()) ?? null;
		}
		case "get_label": {
			return (await requireSession(request.key).getLabel(request.entryId)) ?? null;
		}
		case "get_leaf_id": {
			return (await requireSession(request.key).getLeafId()) ?? null;
		}
		case "get_metadata": {
			return normalizeMetadata(await requireSession(request.key).getMetadata());
		}
		case "files": {
			const files = listFiles(root);
			const normalized: Record<string, string> = {};
			for (const [path, content] of Object.entries(files)) {
				normalized[normalizePath(`/${path}`)] = content.split(root).join("<root>");
			}
			return normalized;
		}
		case "read_file": {
			return readFileSync(join(root, request.path), "utf8");
		}
		case "context": {
			const options: any = {};
			if (request.dropCompaction) {
				options.entryTransforms = [(entries: any[]) => entries.filter((entry) => entry.type !== "compaction")];
			}
			if (request.projectNote) {
				options.entryProjectors = {
					note: (entry: any) => [
						{
							role: "user",
							content: [{ type: "text", text: `note: ${String(entry.data)}` }],
							timestamp: entry.timestamp,
						},
					],
				};
			}
			const context = sessionApi.buildSessionContext(request.entries, options);
			return context;
		}
		case "search": {
			const searchOptions: any = {};
			if (request.entryTypes !== undefined) searchOptions.entryTypes = request.entryTypes;
			if (request.limit !== undefined) searchOptions.limit = request.limit;
			if (request.aborted) {
				const controller = new AbortController();
				const reason = new Error("The operation was aborted");
				reason.name = "AbortError";
				controller.abort(reason);
				searchOptions.signal = controller.signal;
			}
			let search: any;
			if (request.source === "jsonl") {
				const query = request.cwd === undefined ? {} : { cwd: resolve(root, request.cwd) };
				const source = async function* () {
					for (const metadata of await repoApi.listJsonlSessionMetadata({ fs: env, sessionsRoot: root }, query)) {
						yield await repoApi.loadJsonlSessionStorage({ fs: env, sessionsRoot: root }, metadata);
					}
				};
				search = searchApi.createScanningSessionSearch(source);
			} else {
				const readables = request.keys.map((key: string) => requireSession(key));
				search = searchApi.createScanningSessionSearch(readables);
			}
			const hits: any[] = [];
			for await (const hit of search.search(request.text, searchOptions)) hits.push(hit);
			return hits;
		}
		case "scan_entries": {
			const options = request.pageSize === undefined ? {} : { pageSize: request.pageSize };
			const candidates: any[] = [];
			for await (const candidate of searchApi.scanningEntries(requireSession(request.key), options)) {
				candidates.push(candidate);
			}
			return candidates;
		}
		case "codec": {
			return {
				encodedHeaders: (request.headers ?? []).map((header: any) => codecApi.encodeHeader(header)),
				parsedHeaders: (request.headers ?? []).map((header: any) => codecApi.parseHeader(JSON.stringify(header))),
				encodedMutations: (request.mutations ?? []).map((mutation: any) => codecApi.encodeMutation(mutation)),
				parsedMutations: (request.mutationLines ?? []).map((line: string) => codecApi.parseMutation(line)),
			};
		}
		default:
			throw new Error(`Unsupported operation: ${request.op}`);
	}
}

function respond(response: OracleResponse): void {
	process.stdout.write(`${JSON.stringify(response)}\n`);
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("session oracle requires the source module path");
	const moduleDir = dirname(modulePath);
	sessionApi = await import(pathToFileURL(modulePath).href);
	repoApi = await import(pathToFileURL(join(moduleDir, "jsonl/repo.ts")).href);
	envApi = await import(pathToFileURL(join(moduleDir, "../env/nodejs.ts")).href);
	searchApi = await import(pathToFileURL(join(moduleDir, "../../search/index.ts")).href);
	codecApi = await import(pathToFileURL(join(moduleDir, "jsonl/codec.ts")).href);
	const readline = await import("node:readline");
	const reader = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of reader) {
		if (!line.trim()) continue;
		let request: OracleRequest;
		try {
			request = JSON.parse(line) as OracleRequest;
		} catch {
			continue;
		}
		try {
			respond({ id: request.id, ok: true, output: await execute(request) });
		} catch (error) {
			respond({ id: request.id, ok: false, error: normalizeError(error) });
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
	process.exit(1);
});
