import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ExtensionWire = {
	path: string;
	sourceInfo: { scope: string; source: string };
	commands: number;
	tools: number;
	handlers: number;
};

type ListExtensionsApi = {
	formatExtensionInventory(extensions: unknown[]): string;
};

type Request = {
	id: RequestId;
	operation: "format";
	extensions: ExtensionWire[];
};

type Response =
	| { id: RequestId; ok: true; output: string }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function toExtension(wire: ExtensionWire): unknown {
	return {
		path: wire.path,
		sourceInfo: { scope: wire.sourceInfo.scope, source: wire.sourceInfo.source },
		commands: new Map(Array.from({ length: wire.commands }, (_, index) => [`c${index}`, {}])),
		tools: new Map(Array.from({ length: wire.tools }, (_, index) => [`t${index}`, {}])),
		handlers: new Map(Array.from({ length: wire.handlers }, (_, index) => [`h${index}`, [{}]])),
	};
}

function execute(request: Request, api: ListExtensionsApi): Response {
	try {
		return {
			id: request.id,
			ok: true,
			output: api.formatExtensionInventory(request.extensions.map(toExtension)),
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: ListExtensionsApi): void {
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

function serveRequests(api: ListExtensionsApi): void {
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
		throw new Error("list-extensions oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ListExtensionsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`list-extensions oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
