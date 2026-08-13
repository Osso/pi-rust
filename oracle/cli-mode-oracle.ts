import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;
type CliMode = "text" | "json" | "rpc";

type Request = {
	id: RequestId;
	operation: "cli_mode";
	modes: CliMode[];
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { values: CliMode[] };
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

function execute(request: Request): Response {
	if (request.operation !== "cli_mode") {
		throw new Error(`unsupported operation: ${request.operation}`);
	}
	return { id: request.id, ok: true, output: { values: request.modes } };
}

function executeRequestLine(line: string): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed));
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("CLI mode oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	await import(pathToFileURL(readSourceModulePath()).href);
	serveRequests();
}

void main().catch((error: unknown) => {
	process.stderr.write(`CLI mode oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
