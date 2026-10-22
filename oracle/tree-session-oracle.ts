import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const epoch = 1700000000000;
const RealDate = Date;
class FixtureDate extends RealDate {
	constructor(value?: string | number) {
		super(value === undefined ? epoch : value);
	}
	static now(): number {
		return epoch;
	}
}
Object.defineProperty(globalThis, "Date", { value: FixtureDate });
Object.defineProperty(globalThis.crypto, "getRandomValues", {
	value: (bytes: Uint8Array) => {
		bytes.fill(0);
		return bytes;
	},
});

type RecordValue = Record<string, unknown>;
type Operation = RecordValue & { op: string };
type Request = RecordValue & {
	kind: string;
	entries?: unknown[];
	metadata?: RecordValue;
	operations?: Operation[];
	root?: string;
	fileContent?: string;
	mode?: string;
};

function errorValue(error: unknown): RecordValue {
	if (!(error instanceof Error))
		return { name: "UnknownError", message: String(error) };
	const value = error as Error & {
		code?: string;
		cause?: Error & { code?: string };
	};
	return {
		name: value.name,
		code: value.code,
		message: value.message,
		...(value.cause
			? {
					cause: {
						name: value.cause.name,
						code: value.cause.code,
						message: value.cause.message,
					},
				}
			: {}),
	};
}

async function executeStorageOperation(
	storage: any,
	operation: Operation,
): Promise<unknown> {
	switch (operation.op) {
		case "append":
			return storage.appendEntry(operation.entry);
		case "leaf":
			return storage.getLeafId();
		case "setLeaf":
			return storage.setLeafId(operation.id);
		case "createId":
			return storage.createEntryId();
		case "entry":
			return storage.getEntry(operation.id);
		case "entries":
			return storage.getEntries();
		case "find":
			return storage.findEntries(operation.type);
		case "label":
			return storage.getLabel(operation.id);
		case "path":
			return storage.getPathToRoot(operation.id);
		case "metadata":
			return storage.getMetadata();
		default:
			throw new Error(`unknown storage operation ${operation.op}`);
	}
}

async function executeFacadeOperation(
	session: any,
	operation: Operation,
): Promise<unknown> {
	const a = (operation.args as unknown[]) ?? [];
	switch (operation.op) {
		case "appendMessage":
			return session.appendMessage(...a);
		case "thinking":
			return session.appendThinkingLevelChange(...a);
		case "model":
			return session.appendModelChange(...a);
		case "tools":
			return session.appendActiveToolsChange(...a);
		case "compaction":
			return session.appendCompaction(...a);
		case "custom":
			return session.appendCustomEntry(...a);
		case "customMessage":
			return session.appendCustomMessageEntry(...a);
		case "appendLabel":
			return session.appendLabel(...a);
		case "name":
			return session.appendSessionName(...a);
		case "getName":
			return session.getSessionName();
		case "move":
			return session.moveTo(...a);
		case "branch":
			return session.getBranch(...a);
		case "context":
			return session.buildContext();
		default:
			return executeStorageOperation(session.getStorage(), operation);
	}
}

async function exerciseRepository(
	request: Request,
	sourceUrl: URL,
): Promise<unknown> {
	const cwd = request.cwd as string;
	const jsonl = request.mode === "jsonl";
	const module = await import(
		new URL(jsonl ? "./jsonl-repo.ts" : "./memory-repo.ts", sourceUrl).href
	);
	const { NodeExecutionEnv } = await import(
		new URL("../env/nodejs.ts", sourceUrl).href
	);
	const repo = jsonl
		? new module.JsonlSessionRepo({
				fs: new NodeExecutionEnv({ cwd: request.root }),
				sessionsRoot: join(request.root!, "sessions"),
			})
		: new module.InMemorySessionRepo();
	const options = (id: string) => ({ id, ...(jsonl ? { cwd } : {}) });
	const snapshot = async (session: any) => ({
		metadata: await session.getMetadata(),
		entries: await session.getEntries(),
		leaf: await session.getLeafId(),
	});
	const empty = await repo.list();
	const source = await repo.create(options("source"));
	for (const entry of request.entries ?? [])
		await source.getStorage().appendEntry(entry);
	const metadata = await source.getMetadata();
	const original = await snapshot(source);
	const forks = [];
	for (const [id, entryId, position] of [
		["all", undefined, "before"],
		["before", "child", "before"],
		["at", "custom", "at"],
		["invalid", "custom", "before"],
		["missing", "missing", "at"],
		["empty", "", "before"],
	]) {
		try {
			forks.push(
				await snapshot(
					await repo.fork(metadata, { ...options(id!), entryId, position }),
				),
			);
		} catch (error) {
			forks.push({ error: errorValue(error) });
		}
	}
	const reopened = await snapshot(await repo.open(metadata));
	const listed = await repo.list();
	const filtered = await repo.list({ cwd });
	await repo.delete(metadata);
	await repo.delete(metadata);
	let missing;
	try {
		await repo.open(metadata);
	} catch (error) {
		missing = errorValue(error);
	}
	const afterDelete = await repo.list();
	const replaced = await snapshot(await repo.create(options("all")));
	const afterReplace = await repo.list();
	return {
		empty,
		original,
		forks,
		reopened,
		listed,
		filtered,
		missing,
		afterDelete,
		replaced,
		afterReplace,
	};
}

async function main(): Promise<void> {
	const sourceUrl = pathToFileURL(process.argv[2]!);
	const [sessions, memory, jsonl, env, forks, uuid] = await Promise.all([
		import(sourceUrl.href),
		import(new URL("./memory-storage.ts", sourceUrl).href),
		import(new URL("./jsonl-storage.ts", sourceUrl).href),
		import(new URL("../env/nodejs.ts", sourceUrl).href),
		import(new URL("./repo-utils.ts", sourceUrl).href),
		import(new URL("./uuid.ts", sourceUrl).href),
	]);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(JSON.parse(line).wire) as Request;
			let result: unknown;
			if (request.kind === "repository") {
				result = await exerciseRepository(request, sourceUrl);
			} else if (request.kind === "uuidSamples") {
				result = Array.from({ length: Number(request.count) }, () =>
					uuid.uuidv7(),
				);
			} else if (request.kind === "context") {
				result = sessions.buildSessionContext(request.entries ?? []);
			} else {
				let storage: any;
				if (request.mode === "jsonl") {
					const root = request.root!;
					mkdirSync(root, { recursive: true });
					const fs = new env.NodeExecutionEnv({ cwd: root });
					const path = join(root, "session.jsonl");
					if (request.fileContent !== undefined) {
						writeFileSync(path, request.fileContent);
						storage = await jsonl.JsonlSessionStorage.open(fs, path);
					} else {
						storage = await jsonl.JsonlSessionStorage.create(fs, path, {
							cwd: root,
							sessionId: "fixture",
							parentSessionPath: request.parentSessionPath,
						});
					}
				} else {
					storage = new memory.InMemorySessionStorage({
						entries: request.entries,
						metadata: request.metadata,
					});
				}
				if (request.kind === "fork") {
					result = await forks.getEntriesToFork(storage, request.options ?? {});
				} else {
					const session = new sessions.Session(storage);
					const logs = [];
					for (const operation of request.operations ?? []) {
						try {
							const value =
								request.kind === "facade"
									? await executeFacadeOperation(session, operation)
									: await executeStorageOperation(storage, operation);
							logs.push({ op: operation.op, value });
						} catch (error) {
							logs.push({ op: operation.op, error: errorValue(error) });
						}
					}
					result = logs;
				}
			}
			process.stdout.write(
				`${JSON.stringify({ wire: JSON.stringify({ value: result }) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ wire: JSON.stringify({ error: errorValue(error) }) })}\n`,
			);
		}
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
