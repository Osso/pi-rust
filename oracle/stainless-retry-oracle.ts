import { createInterface } from "node:readline";
import { resolve, dirname } from "node:path";
import { pathToFileURL } from "node:url";

async function observe(sdk: string, headers: Record<string, string>): Promise<unknown> {
	const root = resolve(dirname(process.argv[2]), "../../../..");
	const file = sdk === "openai" ? "openai/client.mjs" : "@anthropic-ai/sdk/client.mjs";
	const module = await import(pathToFileURL(resolve(root, "node_modules", file)).href);
	const timeout = 999123;
	const delays: number[] = [];
	let calls = 0;
	const originalTimeout = globalThis.setTimeout;
	const watchdog = originalTimeout(() => {
		console.error(`Retry fixture exceeded 3s: ${sdk}`);
		process.exit(1);
	}, 3000);
	globalThis.setTimeout = ((callback: (...args: unknown[]) => void, ms?: number, ...args: unknown[]) => {
		if (ms === timeout) return originalTimeout(callback, ms, ...args);
		delays.push(ms ?? 0);
		return originalTimeout(callback, 0, ...args);
	}) as typeof setTimeout;
	try {
		const Client = sdk === "openai" ? module.OpenAI : module.Anthropic;
		const client = new Client({
			apiKey: "test", maxRetries: 1, timeout,
			fetch: async () => {
				calls++;
				return calls === 1
					? new Response(JSON.stringify({ error: { message: "retry" } }), { status: 503, headers })
					: new Response(JSON.stringify({ content: [], choices: [] }), { status: 200, headers: { "content-type": "application/json" } });
			},
		});
		if (sdk === "openai") {
			await client.chat.completions.create({ model: "test", messages: [] });
		} else {
			await client.messages.create({ model: "test", messages: [], max_tokens: 1 });
		}
		return { calls, delays };
	} finally {
		globalThis.setTimeout = originalTimeout;
		clearTimeout(watchdog);
	}
}

async function main(): Promise<void> {
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		process.stdout.write(`${JSON.stringify(await observe(request.sdk, request.headers))}\n`);
	}
}
void main().catch((error) => {
	console.error(error);
	process.exit(1);
});
