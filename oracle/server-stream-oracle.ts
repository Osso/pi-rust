import { createInterface } from "node:readline";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { encodeServerMessage, type ServerMessage } from "@earendil-works/pi-protocol";

type RequestId = string | number | null;

type Step = { sendHex: string };

type Request = {
	id: RequestId;
	serverId: string;
	settleMs: number;
	steps: Step[];
};

type Response =
	| { id: RequestId; ok: true; output: { steps: string[][] } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

type ServerModule = {
	createUnixServer(
		service: unknown,
		options: { path: string; serverId: string },
	): {
		start(): Promise<unknown>;
		close(): Promise<void>;
	};
};
type ServiceModule = { TestServerService: new () => { seed(id: string): void } };
type ClientModule = {
	connectUnixTestClient(path: string): Promise<{
		sendBytes(chunk: Uint8Array): Promise<void>;
		close(): Promise<void>;
		messages: ServerMessage[];
	}>;
};

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function normalize(value: unknown, key?: string): unknown {
	if (typeof value === "string") {
		if (key === "connectionId") return "<connection>";
		return UUID_PATTERN.test(value) ? "<uuid>" : value;
	}
	if (Array.isArray(value)) return value.map((entry) => normalize(entry));
	if (value !== null && typeof value === "object") {
		const result: Record<string, unknown> = {};
		for (const [entryKey, entry] of Object.entries(value)) result[entryKey] = normalize(entry, entryKey);
		return result;
	}
	return value;
}

function toHex(bytes: Uint8Array): string {
	return Buffer.from(bytes).toString("hex");
}

function fromHex(value: string): Uint8Array {
	return Buffer.from(value, "hex");
}

function delay(milliseconds: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function settle(client: { messages: ServerMessage[] }, index: number, settleMs: number): Promise<ServerMessage[]> {
	let lastLength = -1;
	for (;;) {
		await delay(settleMs);
		const length = client.messages.length;
		if (length === lastLength) return client.messages.slice(index);
		lastLength = length;
	}
}

async function loadModules(serverModulePath: string): Promise<{
	server: ServerModule;
	service: ServiceModule;
	client: ClientModule;
}> {
	const server = (await import(pathToFileURL(serverModulePath).href)) as ServerModule;
	const service = (await import(
		pathToFileURL(serverModulePath.replace("transports/unix/preset.ts", "testing/service.ts")).href
	)) as ServiceModule;
	const client = (await import(
		pathToFileURL(serverModulePath.replace("transports/unix/preset.ts", "testing/client.ts")).href
	)) as ClientModule;
	return { server, service, client };
}

async function execute(
	request: Request,
	modules: Awaited<ReturnType<typeof loadModules>>,
): Promise<Response> {
	const directory = await mkdtemp(join(tmpdir(), "pss-diff-"));
	const socketPath = join(directory, "server.sock");
	const service = new modules.service.TestServerService();
	service.seed("session-1");
	service.seed("session-2");
	const server = modules.server.createUnixServer(service, {
		path: socketPath,
		serverId: request.serverId,
	});
	await server.start();
	const client = await modules.client.connectUnixTestClient(socketPath);
	try {
		const steps: string[][] = [];
		for (const step of request.steps) {
			const index = client.messages.length;
			await client.sendBytes(fromHex(step.sendHex));
			const messages = await settle(client, index, request.settleMs);
			steps.push(
				messages
					.map((message) => toHex(encodeServerMessage(normalize(message) as ServerMessage)))
					.sort(),
			);
		}
		return { id: request.id, ok: true, output: { steps } };
	} finally {
		await client.close();
		await server.close();
		await rm(directory, { recursive: true, force: true });
	}
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("server stream oracle requires a source module path");
	const modules = await loadModules(sourceModulePath);
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify(await execute(request, modules))}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`server stream oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
