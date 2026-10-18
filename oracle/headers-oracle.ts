import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request =
	| { id: number; operation: "headers"; pairs: Array<[string, string]> }
	| { id: number; operation: "provider"; headers: Record<string, string | null> | null };

type HeadersApi = {
	headersToRecord(headers: Headers): Record<string, string>;
	providerHeadersToRecord(headers: Record<string, string | null> | undefined): Record<string, string> | undefined;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: HeadersApi) {
	try {
		const output =
			request.operation === "headers"
				? { record: api.headersToRecord(new Headers(request.pairs)) }
				: { record: api.providerHeadersToRecord(request.headers ?? undefined) ?? null };
		return { id: request.id, ok: true, output };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: HeadersApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			process.stdout.write(`${JSON.stringify(execute(JSON.parse(buffer.slice(0, lineFeed)), api))}\n`);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("headers oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as HeadersApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`headers oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
