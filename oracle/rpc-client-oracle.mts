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
};
type Api = {
	RpcClient: new (options: {
		cliPath: string;
		env: Record<string, string>;
	}) => Client;
};
type Request = { operation: "send" | "collect" | "idle"; reply: unknown };

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
		env: { PI_RPC_CLIENT_REPLY: JSON.stringify(request.reply) },
	});
	try {
		await client.start();
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

async function main() {
	const api = (await import(pathToFileURL(process.argv[2]).href)) as Api;
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		const request = JSON.parse(line) as Request;
		process.stdout.write(JSON.stringify(await execute(api, request)) + "\n");
	}
}
await main();
