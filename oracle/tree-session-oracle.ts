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
			if (request.kind === "uuidSamples") {
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
