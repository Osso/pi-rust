import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Model = Record<string, unknown>;

type AliasApi = Record<string, (model: Model, context: unknown, options?: unknown) => any>;

type Request = { id: RequestId; op: "aliases" };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

const CODEX_TOKEN =
	"eyJhbGciOiJub25lIn0.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdF90ZXN0In19.sig";

const CASES: Array<{
	alias: string;
	api: string;
	provider: string;
	baseUrl: string;
	token?: string;
	extra?: Record<string, unknown>;
}> = [
	{ alias: "Anthropic", api: "anthropic-messages", provider: "anthropic", baseUrl: "https://api.anthropic.com" },
	{
		alias: "AzureOpenAIResponses",
		api: "azure-openai-responses",
		provider: "azure-openai-responses",
		baseUrl: "https://example.test/openai",
	},
	{ alias: "Google", api: "google-generative-ai", provider: "google", baseUrl: "https://example.test" },
	{ alias: "GoogleVertex", api: "google-vertex", provider: "google-vertex", baseUrl: "https://example.test" },
	{ alias: "Mistral", api: "mistral-conversations", provider: "mistral", baseUrl: "https://example.test" },
	{
		alias: "OpenAICodexResponses",
		api: "openai-codex-responses",
		provider: "openai-codex",
		baseUrl: "https://chatgpt.com/backend-api",
		token: CODEX_TOKEN,
		extra: { transport: "sse" },
	},
	{ alias: "OpenAICompletions", api: "openai-completions", provider: "openai", baseUrl: "https://example.test/v1" },
	{ alias: "OpenAIResponses", api: "openai-responses", provider: "openai", baseUrl: "https://example.test/v1" },
];

function aliasName(testCase: (typeof CASES)[number], simple: boolean): string {
	return simple ? `streamSimple${testCase.alias}` : `stream${testCase.alias}`;
}

function model(testCase: (typeof CASES)[number]): Model {
	return {
		id: "test-model",
		name: "Test Model",
		api: testCase.api,
		provider: testCase.provider,
		baseUrl: testCase.baseUrl,
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 128000,
		maxTokens: 4096,
	};
}

const context = { messages: [{ role: "user", content: "hi", timestamp: 0 }] };

async function execute(aliases: AliasApi) {
	const results = [];
	for (const testCase of CASES) {
		for (const simple of [false, true]) {
			const name = aliasName(testCase, simple);
			const alias = aliases[name];
			const requests: Array<{ url: string; method: string }> = [];
			const fetchImpl = async (input: unknown, init?: RequestInit) => {
				requests.push({ url: String(input), method: init?.method ?? "GET" });
				return new Response(JSON.stringify({ error: { message: "recorded" } }), {
					status: 400,
					headers: { "content-type": "application/json" },
				});
			};
			const originalFetch = globalThis.fetch;
			globalThis.fetch = fetchImpl as typeof fetch;
			try {
				const options = {
					apiKey: testCase.token ?? "test-key",
					...(testCase.extra ?? {}),
				};
				const result = await alias(model(testCase), context, options).result();
				results.push({
					name,
					requests,
					stopReason: result.stopReason ?? null,
					errorMessage: result.errorMessage ?? null,
					error: null,
				});
			} catch (error) {
				results.push({ name, requests, stopReason: null, errorMessage: null, error: normalizeError(error) });
			} finally {
				globalThis.fetch = originalFetch;
			}
		}
	}
	return { results };
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("missing module path");
	const aliases = (await import(pathToFileURL(modulePath).href)) as AliasApi;
	const readline = await import("node:readline");
	const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of lines) {
		if (line.trim().length === 0) continue;
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const output = await execute(aliases);
			console.log(JSON.stringify({ id, ok: true, output }));
		} catch (error) {
			console.log(JSON.stringify({ id, ok: false, error: normalizeError(error) }));
		}
	}
}

main().catch((error: unknown) => {
	console.error(error);
	process.exit(1);
});

