import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type FuzzyMatchApi = {
	fuzzyMatch(query: string, text: string): { matches: boolean; score: number };
	fuzzyFilter<T>(items: T[], query: string, getText: (item: T) => string | string[]): T[];
};

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

function requireString(request: Request, key: string): string {
	const value = request[key];
	if (typeof value !== "string") throw new Error(`${request.operation} requires string field ${key}`);
	return value;
}

function requireArray<T>(request: Request, key: string): T[] {
	const value = request[key];
	if (!Array.isArray(value)) throw new Error(`${request.operation} requires array field ${key}`);
	return value as T[];
}

function f64Bits(value: number): string {
	const buffer = new ArrayBuffer(8);
	new DataView(buffer).setFloat64(0, value, false);
	return Array.from(new Uint8Array(buffer), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "fuzzy_match": {
			const api = (await loadApi(sourceDirectory, "fuzzy.ts")) as FuzzyMatchApi;
			const cases = requireArray<{ query: string; text: string }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					const result = api.fuzzyMatch(testCase.query, testCase.text);
					return { matches: result.matches, scoreBits: f64Bits(result.score) };
				}),
			};
		}
		case "fuzzy_filter": {
			const api = (await loadApi(sourceDirectory, "fuzzy.ts")) as FuzzyMatchApi;
			const cases = requireArray<{ query: string; items: string[][] }>(request, "cases");
			return {
				results: cases.map((testCase) =>
					api.fuzzyFilter(testCase.items, testCase.query, (candidates) => candidates),
				),
			};
		}
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
		const response: Response = { id, ok: true, output };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
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
		throw new Error("tui leaves oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
