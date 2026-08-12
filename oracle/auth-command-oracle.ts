import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;
type AuthCommandKind = "check" | "api_key" | "bearer_token";

type AuthCommandApi = {
	getAuthCommandName(kind: AuthCommandKind): string;
	getAuthCommandUsage(kind: AuthCommandKind): string;
	isAuthCommandHelp(args: string[]): boolean;
};

type Request =
	| {
			id: RequestId;
			operation: "metadata";
			kind: AuthCommandKind;
		}
	| {
			id: RequestId;
			operation: "is_help";
			args: string[];
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
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: AuthCommandApi): Response {
	try {
		switch (request.operation) {
			case "metadata":
				return {
					id: request.id,
					ok: true,
					output: {
						kind: request.kind,
						name: api.getAuthCommandName(request.kind),
						usage: api.getAuthCommandUsage(request.kind),
					},
				};
			case "is_help":
				return { id: request.id, ok: true, output: api.isAuthCommandHelp(request.args) };
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: AuthCommandApi): void {
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

function serveRequests(api: AuthCommandApi): void {
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
		throw new Error("auth command oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as AuthCommandApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`auth command oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
