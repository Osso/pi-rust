import { pathToFileURL } from "node:url";

type RequestId = string | number | null;
type NamedText = { name: string; text: string };

type Request =
	| { id: RequestId; operation: "repair_json"; cases: NamedText[] }
	| { id: RequestId; operation: "parse_json_with_repair"; cases: NamedText[] }
	| { id: RequestId; operation: "parse_streaming_json"; cases: Array<{ name: string; text: string | null }> };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: Record<string, unknown>): Response {
	if (request.operation === "repair_json") {
		const repairJson = api.repairJson as (text: string) => string;
		return {
			id: request.id,
			ok: true,
			output: { results: request.cases.map((item) => ({ name: item.name, repaired: repairJson(item.text) })) },
		};
	}
	if (request.operation === "parse_json_with_repair") {
		const parseJsonWithRepair = api.parseJsonWithRepair as (text: string) => unknown;
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => {
					try {
						return { name: item.name, ok: true, value: parseJsonWithRepair(item.text) };
					} catch {
						return { name: item.name, ok: false };
					}
				}),
			},
		};
	}
	if (request.operation === "parse_streaming_json") {
		const parseStreamingJson = api.parseStreamingJson as (text: string | undefined) => unknown;
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => ({
					name: item.name,
					value: parseStreamingJson(item.text ?? undefined),
				})),
			},
		};
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
	if (modulePath === undefined) throw new Error("json-parse oracle requires a source module path");
	const api = (await import(pathToFileURL(`${modulePath}/utils/json-parse.ts`).href)) as Record<string, unknown>;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`json-parse oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
