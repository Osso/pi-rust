import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

export type OracleRequest = {
	id: RequestId;
	operation: string;
	cases?: unknown[];
	[key: string]: unknown;
};

export type OracleHandler = (api: Record<string, unknown>, request: OracleRequest) => unknown;

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function executeRequestLine(line: string, api: Record<string, unknown>, handler: OracleHandler): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as OracleRequest;
		id = request.id;
		const output = handler(api, request);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output } satisfies Response)}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) } satisfies Response)}\n`);
	}
}

function serveRequests(api: Record<string, unknown>, handler: OracleHandler): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api, handler);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

export async function runOracle(handler: OracleHandler): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("coding-agent utils oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as Record<string, unknown>;
	serveRequests(api, handler);
}

export function requireCases(request: OracleRequest): unknown[] {
	if (!Array.isArray(request.cases)) {
		throw new Error(`operation ${request.operation} requires cases`);
	}
	return request.cases;
}
