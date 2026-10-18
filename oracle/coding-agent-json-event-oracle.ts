import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type JsonEventApi = {
	toJsonEvent: (event: unknown) => unknown;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function executeRequestLine(line: string, api: JsonEventApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as { id: RequestId; events: unknown[] };
		id = request.id;
		const events = request.events.map((event) => api.toJsonEvent(event));
		const response: Response = { id, ok: true, output: { events } };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: JsonEventApi): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("json-event oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as JsonEventApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`json-event oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
