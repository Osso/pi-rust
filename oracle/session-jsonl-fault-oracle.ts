import type {
	FileSystem,
	FileErrorCode,
	JsonlSessionMetadata,
	SessionStorage,
	SessionTreeEntry,
} from "/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/agent-core/src/harness/types.ts";
type StorageApi =
	typeof import("/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/agent-core/src/harness/session/jsonl-storage.ts");
type RepositoryApi =
	typeof import("/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/agent-core/src/harness/session/jsonl-repo.ts");
type ErrorApi =
	typeof import("/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/agent-core/src/harness/types.ts");
interface Input {
	root: string;
	kind: string;
	action: string;
	message: string;
	header: unknown;
	base: SessionTreeEntry[];
	append: SessionTreeEntry;
	fault: {
		method: string;
		nth: number;
		after: boolean;
		rejected: boolean;
		code: FileErrorCode;
	} | null;
}
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
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
const faultMethods = [
	"absolutePath",
	"joinPath",
	"readTextFile",
	"readTextLines",
	"writeFile",
	"appendFile",
	"listDir",
	"exists",
	"createDir",
	"remove",
] as const;

function errorValue(error: unknown): unknown {
	if (!(error instanceof Error)) throw error;
	const value = error as Error & {
		code?: string;
		path?: string;
		cause?: unknown;
	};
	return {
		name: value.name,
		code: value.code,
		message: value.message,
		path: value.path,
		...(value.cause === undefined ? {} : { cause: errorValue(value.cause) }),
	};
}
async function observe(operation: () => Promise<unknown>): Promise<unknown> {
	try {
		await operation();
		return { ok: true };
	} catch (error) {
		return { ok: false, error: errorValue(error) };
	}
}
function readFiles(root: string, prefix = ""): Record<string, string> {
	const output: Record<string, string> = {};
	for (const item of readdirSync(join(root, prefix), { withFileTypes: true })) {
		const relative = prefix ? `${prefix}/${item.name}` : item.name;
		if (item.isDirectory()) Object.assign(output, readFiles(root, relative));
		else if (item.isFile())
			output[relative] = readFileSync(join(root, relative), "utf8");
	}
	return output;
}
async function snapshot(storage: SessionStorage): Promise<unknown> {
	return {
		entries: await storage.getEntries(),
		leaf: await storage.getLeafId(),
		label: (await storage.getLabel("entry")) ?? null,
	};
}
function installFileFaults(
	fs: FileSystem,
	input: Input,
	FileError: ErrorApi["FileError"],
): void {
	let remaining = input.fault?.nth ?? 0;
	for (const method of faultMethods) {
		const original = fs[method];
		Object.defineProperty(fs, method, {
			value: async (...args: unknown[]) => {
				const fault = input.fault;
				if (!fault || fault.method !== method || --remaining !== 0)
					return Reflect.apply(original, fs, args);
				if (fault.rejected) throw new TypeError(input.message);
				const error = {
					ok: false,
					error: new FileError(fault.code, input.message),
				};
				if (!fault.after) return error;
				await Reflect.apply(original, fs, args);
				return error;
			},
		});
	}
}
function storageOperation(
	action: string,
	api: StorageApi,
	fs: FileSystem,
	path: string,
): (() => Promise<unknown>) | undefined {
	switch (action) {
		case "create":
			return () =>
				api.JsonlSessionStorage.create(fs, path, {
					cwd: "/fixture",
					sessionId: "fixture",
				});
		case "open":
			return () => api.JsonlSessionStorage.open(fs, path);
		case "header":
			return () => api.loadJsonlSessionMetadata(fs, path);
		default:
			return undefined;
	}
}
async function runStorageMutation(
	input: Input,
	fs: FileSystem,
	real: FileSystem,
	api: StorageApi,
	path: string,
): Promise<unknown> {
	const storage = await api.JsonlSessionStorage.open(fs, path);
	const operation = () =>
		input.action === "append"
			? storage.appendEntry(input.append)
			: storage.setLeafId(null);
	const attempt = await observe(operation);
	const state = await snapshot(storage);
	const reopened = await snapshot(
		await api.JsonlSessionStorage.open(real, path),
	);
	const diskBeforeRetry = readFileSync(path, "utf8");
	const retry = await observe(operation);
	return {
		attempt,
		state,
		reopened,
		diskBeforeRetry,
		retry,
		afterRetry: await snapshot(storage),
	};
}
async function runStorageCase(
	input: Input,
	fs: FileSystem,
	real: FileSystem,
	api: StorageApi,
): Promise<unknown> {
	const path = join(input.root, "session.jsonl");
	if (input.action !== "create")
		writeFileSync(
			path,
			`${JSON.stringify(input.header)}\n${JSON.stringify(input.base[0])}\n`,
		);
	const operation = storageOperation(input.action, api, fs, path);
	if (operation) return { attempt: await observe(operation) };
	return runStorageMutation(input, fs, real, api, path);
}
function repositoryOperation(
	repo: InstanceType<RepositoryApi["JsonlSessionRepo"]>,
	metadata: JsonlSessionMetadata,
	action: string,
): () => Promise<unknown> {
	switch (action) {
		case "create":
			return () => repo.create({ cwd: "/fixture", id: "new" });
		case "open":
			return () => repo.open(metadata);
		case "delete":
			return () => repo.delete(metadata);
		case "list":
			return () => repo.list();
		case "listCwd":
			return () => repo.list({ cwd: "/fixture" });
		case "fork":
			return () => repo.fork(metadata, { cwd: "/fixture", id: "fork" });
		default:
			return async () => {
				throw new Error("unknown repository action");
			};
	}
}
async function runRepositoryCase(
	input: Input,
	fs: FileSystem,
	real: FileSystem,
	JsonlSessionRepo: RepositoryApi["JsonlSessionRepo"],
): Promise<unknown> {
	const sessionsRoot = join(input.root, "sessions");
	const seed = new JsonlSessionRepo({ fs: real, sessionsRoot });
	const session = await seed.create({ cwd: "/fixture", id: "fixture" });
	for (const entry of input.base) await session.getStorage().appendEntry(entry);
	const metadata = await session.getMetadata();
	const repo = new JsonlSessionRepo({ fs, sessionsRoot });
	return {
		attempt: await observe(repositoryOperation(repo, metadata, input.action)),
	};
}
async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("missing frozen module path");
	const source = pathToFileURL(modulePath);
	const storageApi: StorageApi = await import(source.href);
	const { JsonlSessionRepo } = await import(
		new URL("./jsonl-repo.ts", source).href
	);
	const { NodeExecutionEnv } = await import(
		new URL("../env/nodejs.ts", source).href
	);
	const { FileError } = await import(new URL("../types.ts", source).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input: Input = JSON.parse(line);
		mkdirSync(input.root, { recursive: true });
		const real = new NodeExecutionEnv({ cwd: input.root });
		const fs = new NodeExecutionEnv({ cwd: input.root });
		installFileFaults(fs, input, FileError);
		const result =
			input.kind === "storage"
				? await runStorageCase(input, fs, real, storageApi)
				: await runRepositoryCase(input, fs, real, JsonlSessionRepo);
		console.log(
			JSON.stringify({ ...(result as object), files: readFiles(input.root) }),
		);
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
