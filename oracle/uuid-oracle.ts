import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request = { id: RequestId; operation: "generate"; count: number };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: Record<string, unknown>): Response {
	if (request.operation === "generate") {
		const uuidv7 = api.uuidv7 as () => string;
		const before = Date.now();
		const values = Array.from({ length: request.count }, () => uuidv7());
		return { id: request.id, ok: true, output: { before, after: Date.now(), values } };
	}
	throw new Error(`unsupported operation: ${(request as { operation: string }).operation}`);
}

function executeRequestLine(line: string, api: Record<string, unknown>): void {
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

function serveRequests(api: Record<string, unknown>): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("uuid oracle requires a source module path");
	const api = (await import(pathToFileURL(`${modulePath}/utils/uuid.ts`).href)) as Record<string, unknown>;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`uuid oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
