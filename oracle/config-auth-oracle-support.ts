// Shared JSON-lines loop for the config/auth slice oracles.
// Each request line is `{ "id": ..., ...payload }`; each response is
// `{ "id", "ok": true, "output" }` or `{ "id", "ok": false, "error": { "name", "message", "code"? } }`.
// Requests run sequentially so async handlers observe a deterministic order.
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

export type OracleRequest = { id: unknown; [key: string]: unknown };

export function normalizeError(error: unknown): { name: string; message: string; code?: string } {
	if (error instanceof Error) {
		const code = (error as { code?: unknown }).code;
		return typeof code === "string"
			? { name: error.name, message: error.message, code }
			: { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

export async function importSource<T>(): Promise<T> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("oracle requires a source module path");
	return (await import(pathToFileURL(sourcePath).href)) as T;
}

export function serve(handler: (request: OracleRequest) => unknown): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	const respond = async (line: string) => {
		let id: unknown = null;
		try {
			const request = JSON.parse(line) as OracleRequest;
			id = request.id;
			const output = await handler(request);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output: output ?? null })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
		}
	};
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => respond(line));
		}
	});
}
