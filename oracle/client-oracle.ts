import { StringDecoder } from "node:string_decoder";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ClientModule = {
	PiClient: new (options: unknown) => any;
};

type SupportModule = {
	MemoryByteServer: new () => any;
	baseServerSnapshot: unknown;
	sessionSnapshot(id: string, overrides?: Record<string, unknown>): unknown;
};

type ClientRequest = {
	id: RequestId;
	operation: "client_script";
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

async function runScript(clientModule: ClientModule, support: SupportModule): Promise<unknown> {
	const server = new support.MemoryByteServer();
	server.onMessage((message: any) => {
		if (message.type === "hello") {
			server.send({
				type: "hello",
				version: 1,
				connectionId: "connection-1",
				snapshot: support.baseServerSnapshot,
			});
			return;
		}
		if (message.type !== "request") return;
		switch (message.request.command) {
			case "list":
				server.send({
					type: "response",
					id: message.id,
					ok: true,
					result: { command: "list", sessions: [] },
				});
				return;
			case "attach":
				server.send({
					type: "response",
					id: message.id,
					ok: true,
					result: { command: "attach", session: support.sessionSnapshot("session-1") },
				});
				return;
			case "prompt":
				server.send({
					type: "response",
					id: message.id,
					ok: true,
					result: {
						command: "prompt",
						session: support.sessionSnapshot("session-1", { revision: 2, phase: "turn" }),
					},
				});
				return;
			case "detach":
				server.send({
					type: "response",
					id: message.id,
					ok: true,
					result: { command: "detach", sessionId: "session-1" },
				});
				return;
			default:
				return;
		}
	});

	const states: string[] = [];
	const client = new clientModule.PiClient({
		transportFactory: (handlers: unknown) => server.connect(handlers),
	});
	client.onConnectionStateChange(({ state }: { state: string }) => states.push(state));

	await client.connect();
	const listed = await client.listSessions();
	const handle = await client.attachSession("session-1");
	const observed: number[] = [];
	handle.subscribe((snapshot: { revision: number }) => observed.push(snapshot.revision));

	server.send({
		type: "event",
		event: {
			type: "session_snapshot",
			snapshot: support.sessionSnapshot("session-1", { revision: 3, phase: "idle" }),
		},
	});
	const prompted = await handle.prompt("hello");
	const snapshotAfterPrompt = handle.snapshot;
	await handle.detach();
	const afterDetach = handle.snapshot ?? null;
	client.disconnect();

	return {
		frames: server.sentByClient.map((chunk: Uint8Array) => Buffer.from(chunk).toString("hex")),
		states,
		observed,
		listCount: listed.length,
		promptResultRevision: prompted.revision,
		snapshotAfterPrompt: {
			revision: snapshotAfterPrompt.revision,
			phase: snapshotAfterPrompt.phase,
			attached: snapshotAfterPrompt.attached,
		},
		afterDetach,
		connectionState: client.connectionState,
	};
}

function executeRequestLine(
	line: string,
	clientModule: ClientModule,
	support: SupportModule,
): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as ClientRequest;
		id = request.id;
		if (request.operation !== "client_script") throw new Error("unknown client oracle operation");
		void runScript(clientModule, support).then(
			(output) => process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`),
			(error) => process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`),
		);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(clientModule: ClientModule, support: SupportModule): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), clientModule, support);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("client oracle requires a source module path");
	}
	const supportPath = resolve(dirname(sourceModulePath), "../test/support.ts");
	const clientModule = (await import(pathToFileURL(sourceModulePath).href)) as ClientModule;
	const support = (await import(pathToFileURL(supportPath).href)) as SupportModule;
	serveRequests(clientModule, support);
}

void main().catch((error: unknown) => {
	process.stderr.write(`client oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
