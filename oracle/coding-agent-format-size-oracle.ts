import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type FormatSizeCaseRequest = {
	name: string;
	bytes: number;
};

type Request = {
	id: RequestId;
	operation: string;
	cases?: FormatSizeCaseRequest[];
};

type FormatSizeApi = {
	formatSize(bytes: number): string;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: unknown;
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

function execute(request: Request, api: FormatSizeApi): Response {
	if (request.operation !== "coding_agent_format_size" || request.cases === undefined) {
		throw new Error(`unsupported operation: ${request.operation}`);
	}
	const results = request.cases.map((input) => ({
		name: input.name,
		value: api.formatSize(input.bytes),
	}));
	return { id: request.id, ok: true, output: { results } };
}

function executeRequestLine(line: string, api: FormatSizeApi): void {
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

function serveRequests(api: FormatSizeApi): void {
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
		throw new Error("coding-agent format-size oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as FormatSizeApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`coding-agent format-size oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
