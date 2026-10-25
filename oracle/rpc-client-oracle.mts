import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Client = {
	start(): Promise<void>;
	stop(): Promise<void>;
	send(command: { type: "get_state" }): Promise<unknown>;
	collectEvents(timeout: number): Promise<unknown[]>;
	waitForIdle(timeout: number): Promise<void>;
	prompt(message: string): Promise<void>;
	onEvent(
		listener: (event: { type?: unknown; willRetry?: unknown }) => void,
	): () => void;
	setModel(provider: string, id: string): Promise<unknown>;
	cycleModel(): Promise<unknown>;
	getAvailableModels(): Promise<unknown>;
	newSession(): Promise<unknown>;
	getLastAssistantText(): Promise<unknown>;
};
type Api = {
	RpcClient: new (options: {
		cliPath: string;
		executablePath?: string;
		env: Record<string, string>;
	}) => Client;
};
type Request = {
	operation:
		| "send"
		| "collect"
		| "idle"
		| "typed"
		| "collector_skip"
		| "timeout"
		| "spawn_error";
	reply: unknown;
	executable?: string;
	events?: unknown[];
	delay?: number | "NaN" | "Infinity" | "-Infinity";
	idleTimeout?: boolean;
	listenerFault?: boolean;
	listenerRemovesItself?: boolean;
	method?: "model" | "cycle" | "models" | "new" | "text";
};

async function sendTyped(
	client: Client,
	method: Request["method"],
): Promise<unknown> {
	switch (method) {
		case "model":
			return client.setModel("provider", "id");
		case "cycle":
			return client.cycleModel();
		case "models":
			return client.getAvailableModels();
		case "new":
			return client.newSession();
		case "text":
			return client.getLastAssistantText();
		default:
			throw new Error("unknown test method");
	}
}

// Real private stdio child; no provider or network. Both clients run these exact bytes.
const child = await readFile(
	new URL("./rpc-client-child.mjs", import.meta.url),
	"utf8",
);

async function execute(api: Api, request: Request): Promise<unknown> {
	const dir = await mkdtemp(join(tmpdir(), "rpc-client-oracle-"));
	const path = join(dir, "child.mjs");
	await writeFile(path, child);
	const client = new api.RpcClient({
		cliPath: path,
		executablePath: request.executable,
		env: {
			PI_RPC_CLIENT_REPLY: JSON.stringify(request.reply),
			PI_RPC_EXTRA_EVENTS: JSON.stringify(request.events ?? []),
		},
	});
	let remove: (() => void) | undefined;
	if (request.listenerRemovesItself) remove = client.onEvent(() => remove?.());
	if (request.listenerFault)
		client.onEvent((event) => {
			if (event.type === "agent_end" && event.willRetry)
				throw new Error("fixture callback");
		});
	try {
		if (request.operation === "spawn_error") {
			const first = await startOutcome(client);
			const second = await startOutcome(client);
			await client.stop();
			return { ok: true, value: [first, second, await startOutcome(client)] };
		}
		await client.start();
		if (request.operation === "timeout") {
			const delay =
				typeof request.delay === "string"
					? Number(request.delay)
					: (request.delay ?? 2000);
			return {
				ok: true,
				value: request.idleTimeout
					? await client.waitForIdle(delay)
					: await client.collectEvents(delay),
			};
		}
		if (request.operation === "collector_skip") {
			const events = client.collectEvents(2000);
			const seen: unknown[] = [];
			client.onEvent((event) => {
				seen.push(event);
			});
			await client.prompt("input");
			return { ok: true, value: { events: await events, seen } };
		}
		if (request.operation === "typed")
			return { ok: true, value: await sendTyped(client, request.method) };
		if (request.operation === "send")
			return { ok: true, value: await client.send({ type: "get_state" }) };
		if (request.operation === "collect") {
			const events = client.collectEvents(2000);
			await client.prompt("input");
			return { ok: true, value: await events };
		}
		const idle = client.waitForIdle(2000);
		await client.prompt("input");
		await idle;
		return { ok: true };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { ok: false, error: { name: error.name, message: error.message } };
	} finally {
		await client.stop();
		await rm(dir, { recursive: true, force: true });
	}
}

async function startOutcome(client: Client): Promise<unknown> {
	try {
		await client.start();
		return { ok: true };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return {
			ok: false,
			error: {
				name: error.name,
				message: error.message,
				...("code" in error ? { code: error.code } : {}),
			},
		};
	}
}

async function main() {
	const api = (await import(pathToFileURL(process.argv[2]).href)) as Api;
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		const request = JSON.parse(line) as Request;
		process.stdout.write(JSON.stringify(await execute(api, request)) + "\n");
	}
}
await main();
