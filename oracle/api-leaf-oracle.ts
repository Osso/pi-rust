import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request =
	| { id: RequestId; operation: "cloudflare_constants" }
	| { id: RequestId; operation: "clamp_prompt_cache_key"; cases: Array<{ name: string; key: string | null }> };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: Record<string, unknown>): Response {
	if (request.operation === "cloudflare_constants") {
		return {
			id: request.id,
			ok: true,
			output: {
				workersAi: api.CLOUDFLARE_WORKERS_AI_BASE_URL,
				gatewayCompat: api.CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL,
				gatewayOpenai: api.CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL,
				gatewayAnthropic: api.CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL,
			},
		};
	}
	if (request.operation === "clamp_prompt_cache_key") {
		const clamp = api.clampOpenAIPromptCacheKey as (key: string | undefined) => string | undefined;
		return {
			id: request.id,
			ok: true,
			output: {
				maxLength: api.OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH,
				results: request.cases.map((item) => ({
					name: item.name,
					key: clamp(item.key ?? undefined) ?? null,
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
	if (modulePath === undefined) throw new Error("api leaf oracle requires a source module path");
	const cloudflare = (await import(pathToFileURL(`${modulePath}/api/cloudflare.ts`).href)) as Record<string, unknown>;
	const promptCache = (await import(pathToFileURL(`${modulePath}/api/openai-prompt-cache.ts`).href)) as Record<
		string,
		unknown
	>;
	serveRequests({ ...cloudflare, ...promptCache });
}

void main().catch((error: unknown) => {
	process.stderr.write(`api leaf oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
