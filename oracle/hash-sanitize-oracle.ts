import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type HashCase = { name: string; text: string };
type SanitizeCase = { name: string; units: number[] };

type Request =
	| { id: RequestId; operation: "short_hash"; cases: HashCase[] }
	| { id: RequestId; operation: "sanitize_surrogates"; cases: SanitizeCase[] };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: Record<string, unknown>): Response {
	if (request.operation === "short_hash") {
		const shortHash = api.shortHash as (text: string) => string;
		return {
			id: request.id,
			ok: true,
			output: { results: request.cases.map((item) => ({ name: item.name, hash: shortHash(item.text) })) },
		};
	}
	if (request.operation === "sanitize_surrogates") {
		const sanitizeSurrogates = api.sanitizeSurrogates as (text: string) => string;
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => {
					const sanitized = sanitizeSurrogates(String.fromCharCode(...item.units));
					const units = Array.from({ length: sanitized.length }, (_, index) => sanitized.charCodeAt(index));
					return { name: item.name, units };
				}),
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
	if (modulePath === undefined) throw new Error("hash/sanitize oracle requires a source module path");
	const hash = (await import(pathToFileURL(`${modulePath}/utils/hash.ts`).href)) as Record<string, unknown>;
	const sanitize = (await import(pathToFileURL(`${modulePath}/utils/sanitize-unicode.ts`).href)) as Record<
		string,
		unknown
	>;
	serveRequests({ ...hash, ...sanitize });
}

void main().catch((error: unknown) => {
	process.stderr.write(`hash/sanitize oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
