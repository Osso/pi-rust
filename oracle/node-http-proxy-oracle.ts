import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; target: string; env: Record<string, string> | null };

type ProxyApi = {
	resolveHttpProxyUrlForTarget(target: string, env?: Record<string, string>): URL | undefined;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: ProxyApi) {
	try {
		const resolved = api.resolveHttpProxyUrlForTarget(request.target, request.env ?? undefined);
		return { id: request.id, ok: true, output: { url: resolved?.toString() ?? null } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: ProxyApi): void {
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
	if (modulePath === undefined) throw new Error("proxy oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as ProxyApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`proxy oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
