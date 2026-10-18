import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type Request = {
	id: RequestId;
	operation: string;
	[key: string]: unknown;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

type KeybindingsApi = {
	KeybindingsManager: new (
		definitions: unknown,
		userBindings?: Record<string, string[]>,
	) => {
		getKeys(id: string): string[];
		getConflicts(): Array<{ key: string; keybindings: string[] }>;
		getResolvedBindings(): Record<string, string | string[]>;
	};
	TUI_KEYBINDINGS: Record<string, unknown>;
};

function toUserBindings(value: unknown): Record<string, string[]> {
	const bindings: Record<string, string[]> = {};
	if (!Array.isArray(value)) return bindings;
	for (const entry of value) {
		if (!Array.isArray(entry) || entry.length !== 2) throw new Error("userBindings entries must be pairs");
		bindings[String(entry[0])] = (entry[1] as unknown[]).map(String);
	}
	return bindings;
}

async function keybindingsOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "keybindings.ts")) as KeybindingsApi;
	const manager = new api.KeybindingsManager(api.TUI_KEYBINDINGS, toUserBindings(request.userBindings));
	const ids = Object.keys(api.TUI_KEYBINDINGS);
	const keys: Record<string, string[]> = {};
	for (const id of ids) keys[id] = manager.getKeys(id);
	const resolved = manager.getResolvedBindings();
	const resolvedArrays: Record<string, string[]> = {};
	for (const id of ids) {
		const value = resolved[id];
		resolvedArrays[id] = Array.isArray(value) ? [...value] : value === undefined ? [] : [value];
	}
	return { keys, resolved: resolvedArrays, conflicts: manager.getConflicts() };
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "keybindings":
			return keybindingsOperation(request, sourceDirectory);
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function executeRequestLine(line: string, sourceDirectory: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request, sourceDirectory);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(sourceDirectory: string): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			void executeRequestLine(line, sourceDirectory);
		}
	});
}

function readSourceDirectory(): string {
	const sourceDirectory = process.argv[2];
	if (sourceDirectory === undefined) {
		throw new Error("tui core oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
