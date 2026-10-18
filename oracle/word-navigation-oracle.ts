import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type WordNavigationApi = {
	findWordBackward(text: string, cursor: number): number;
	findWordForward(text: string, cursor: number): number;
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

function requireArray<T>(request: Request, key: string): T[] {
	const value = request[key];
	if (!Array.isArray(value)) throw new Error(`${request.operation} requires array field ${key}`);
	return value as T[];
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

function utf16IndexForByteOffset(text: string, byteOffset: number): number {
	return Buffer.from(text, "utf8").subarray(0, byteOffset).toString("utf8").length;
}

function byteOffsetForUtf16Index(text: string, utf16Index: number): number {
	return Buffer.byteLength(text.slice(0, utf16Index), "utf8");
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "word_navigation": {
			const api = (await loadApi(sourceDirectory, "word-navigation.ts")) as WordNavigationApi;
			const cases = requireArray<{ text: string; cursorBytes: number }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					const cursor = utf16IndexForByteOffset(testCase.text, testCase.cursorBytes);
					return {
						backward: byteOffsetForUtf16Index(testCase.text, api.findWordBackward(testCase.text, cursor)),
						forward: byteOffsetForUtf16Index(testCase.text, api.findWordForward(testCase.text, cursor)),
					};
				}),
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
		throw new Error("word navigation oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
