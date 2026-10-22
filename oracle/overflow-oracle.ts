import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request =
	| { id: number; operation: "check"; message: unknown; contextWindow: number | null }
	| { id: number; operation: "patterns"; samples: string[] };

type OverflowApi = {
	isContextOverflow(message: unknown, contextWindow?: number): boolean;
	getOverflowPatterns(): RegExp[];
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: OverflowApi) {
	try {
		if (request.operation === "patterns") {
			const patterns = api.getOverflowPatterns();
			return {
				id: request.id,
				ok: true,
				output: patterns.map((pattern) => request.samples.map((sample) => pattern.test(sample))),
			};
		}
		const contextWindow = request.contextWindow ?? undefined;
		const output: Record<string, boolean> = { overflow: api.isContextOverflow(request.message, contextWindow) };
		return { id: request.id, ok: true, output };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: OverflowApi): void {
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
	if (modulePath === undefined) throw new Error("overflow oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as OverflowApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`overflow oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
