import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type DefaultThinkingLevelApi = {
	DEFAULT_THINKING_LEVEL: string;
};

type Request = {
	id: RequestId;
	operation: "default_thinking_level";
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { value: string };
		}
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
		};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: DefaultThinkingLevelApi): Response {
	if (request.operation !== "default_thinking_level") {
		throw new Error(`unsupported operation: ${request.operation}`);
	}
	return { id: request.id, ok: true, output: { value: api.DEFAULT_THINKING_LEVEL } };
}

function executeRequestLine(line: string, api: DefaultThinkingLevelApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: DefaultThinkingLevelApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("default thinking level oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const sourceModulePath = readSourceModulePath();
	const api = (await import(pathToFileURL(sourceModulePath).href)) as DefaultThinkingLevelApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`default thinking level oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
