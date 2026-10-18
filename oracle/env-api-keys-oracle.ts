import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; operation: "find" | "key"; provider: string; env: Record<string, string> | null };

type EnvApiKeysApi = {
	findEnvKeys(provider: string, env?: Record<string, string>): string[] | undefined;
	getEnvApiKey(provider: string, env?: Record<string, string>): string | undefined;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: EnvApiKeysApi) {
	try {
		const env = request.env ?? undefined;
		const output =
			request.operation === "find"
				? { keys: api.findEnvKeys(request.provider, env) ?? null }
				: { key: api.getEnvApiKey(request.provider, env) ?? null };
		return { id: request.id, ok: true, output };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: EnvApiKeysApi): void {
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
	if (modulePath === undefined) throw new Error("env api keys oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as EnvApiKeysApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`env api keys oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
