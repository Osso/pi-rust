import { mkdtemp, rm } from "node:fs/promises";
import { createConnection, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ServerModule = {
	createUnixServer(service: unknown, options: { path: string; serverId: string }): {
		start(): Promise<void>;
		close(): Promise<void>;
	};
};

type ServiceModule = {
	TestServerService: new () => {
		seed(id: string): void;
		latestRuntime(id: string): Runtime;
	};
};

type Runtime = {
	finishPrompt(): void;
	abort(): Promise<void>;
	setPhase(phase: string): void;
	emitProgress(progress: unknown): void;
	emitSnapshot(): void;
};

type RuntimeAction =
	| { id: string; action: "finishPrompt" | "abort" | "emitSnapshot" }
	| { id: string; action: "setPhase"; phase: string }
	| { id: string; action: "emitProgress"; progress: unknown };

type Step = { sendHex: string } | { runtime: RuntimeAction };

type Request = {
	id: RequestId;
	serverId: string;
	settleMs: number;
	steps: Step[];
};

type Response =
	| { id: RequestId; ok: true; output: { steps: string[][] } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function delay(milliseconds: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/** Captures raw framed payloads without decoding or re-encoding them. */
class RawFrameLog {
	readonly frames: string[] = [];
	private buffer = Buffer.alloc(0);

	push(chunk: Buffer): void {
		this.buffer = Buffer.concat([this.buffer, chunk]);
		for (;;) {
			if (this.buffer.length < 4) return;
			const length = this.buffer.readUInt32BE(0);
			if (this.buffer.length < 4 + length) return;
			this.frames.push(this.buffer.subarray(4, 4 + length).toString("hex"));
			this.buffer = this.buffer.subarray(4 + length);
		}
	}
}

async function settle(log: RawFrameLog, index: number, settleMs: number): Promise<string[]> {
	let lastLength = -1;
	for (;;) {
		await delay(settleMs);
		const length = log.frames.length;
		if (length === lastLength) return log.frames.slice(index);
		lastLength = length;
	}
}

function writeSocket(socket: Socket, chunk: Buffer): Promise<void> {
	return new Promise<void>((resolve, reject) => {
		socket.write(chunk, (error) => {
			if (error) reject(error);
			else resolve();
		});
	});
}

async function runStep(
	step: Step,
	socket: Socket,
	service: InstanceType<ServiceModule["TestServerService"]>,
): Promise<void> {
	if ("sendHex" in step) {
		await writeSocket(socket, Buffer.from(step.sendHex, "hex"));
		return;
	}
	const runtime = service.latestRuntime(step.runtime.id);
	switch (step.runtime.action) {
		case "finishPrompt":
			runtime.finishPrompt();
			return;
		case "abort":
			await runtime.abort();
			return;
		case "setPhase":
			runtime.setPhase(step.runtime.phase);
			return;
		case "emitProgress":
			runtime.emitProgress(step.runtime.progress);
			return;
		case "emitSnapshot":
			runtime.emitSnapshot();
			return;
	}
}

async function execute(
	request: Request,
	modules: { server: ServerModule; service: ServiceModule },
): Promise<{ steps: string[][] }> {
	const directory = await mkdtemp(join(tmpdir(), "pss-diff-"));
	const socketPath = join(directory, "server.sock");
	const service = new modules.service.TestServerService();
	service.seed("session-1");
	service.seed("session-2");
	const server = modules.server.createUnixServer(service, { path: socketPath, serverId: request.serverId });
	await server.start();
	const socket = createConnection(socketPath);
	await new Promise<void>((resolve, reject) => {
		socket.once("connect", resolve);
		socket.once("error", reject);
	});
	const log = new RawFrameLog();
	socket.on("data", (chunk: Buffer) => log.push(chunk));
	try {
		const steps: string[][] = [];
		for (const step of request.steps) {
			const index = log.frames.length;
			await runStep(step, socket, service);
			steps.push(await settle(log, index, request.settleMs));
		}
		return { steps };
	} finally {
		socket.destroy();
		await server.close();
		await rm(directory, { recursive: true, force: true });
	}
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("server stream oracle requires a source module path");
	const server = (await import(pathToFileURL(sourceModulePath).href)) as ServerModule;
	const service = (await import(
		pathToFileURL(sourceModulePath.replace("transports/unix/preset.ts", "testing/service.ts")).href
	)) as ServiceModule;
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const output = await execute(request, { server, service });
			const response: Response = { id, ok: true, output };
			process.stdout.write(`${JSON.stringify(response)}\n`);
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
