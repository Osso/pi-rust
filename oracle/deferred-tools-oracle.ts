import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; enabled: boolean; normalize: "identity" | "lower"; context: unknown };

type DeferredApi = {
	splitDeferredTools(
		context: unknown,
		enabled: boolean,
		normalizeName?: (name: string) => string,
	): { immediate: unknown[]; deferred: Map<string, unknown> };
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: DeferredApi) {
	try {
		const normalize = request.normalize === "lower" ? (name: string) => name.toLowerCase() : undefined;
		const split = api.splitDeferredTools(request.context, request.enabled, normalize);
		return {
			id: request.id,
			ok: true,
			output: { immediate: split.immediate, deferred: [...split.deferred.entries()] },
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: DeferredApi): void {
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
	if (modulePath === undefined) throw new Error("deferred tools oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as DeferredApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`deferred tools oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
