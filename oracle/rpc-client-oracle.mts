import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { constants as osConstants, tmpdir } from "node:os";
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
	promptAndWait(
		message: string,
		images: undefined,
		timeout: number,
	): Promise<unknown[]>;
	onEvent(
		listener: (event: { type?: unknown; willRetry?: unknown }) => void,
	): () => void;
	setModel(provider: string, id: string): Promise<unknown>;
	cycleModel(): Promise<unknown>;
	getAvailableModels(): Promise<unknown>;
	newSession(): Promise<unknown>;
	getLastAssistantText(): Promise<unknown>;
	getStderr(): string;
	getState(): Promise<unknown>;
	getEntries(): Promise<unknown>;
	getTree(): Promise<unknown>;
	getForkMessages(): Promise<unknown>;
};
type Api = {
	RpcClient: new (options: {
		cliPath: string;
		executablePath?: string;
		cwd?: string;
		nodeArgs?: string[];
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
		| "spawn_error"
		| "stop_fail"
		| "stop_writing"
		| "blocked_write"
		| "pipe_error"
		| "promise_lifetime"
		| "prompt_fail_lifetime"
		| "startup_signal"
		| "signal_names";
	reply: unknown;
	signal?: string;
	executable?: string;
	cliPath?: string;
	cwd?: string;
	nodeArgs?: string[];
	envOverrides?: Record<string, string>;
	events?: unknown[];
	delay?: number | "NaN" | "Infinity" | "-Infinity";
	idleTimeout?: boolean;
	listenerFault?: boolean;
	keepWriting?: boolean;
	blockInput?: boolean;
	closeInput?: boolean;
	missingPid?: boolean;
	listenerRemovesItself?: boolean;
	method?:
		| "model"
		| "cycle"
		| "models"
		| "new"
		| "text"
		| "entries"
		| "tree"
		| "fork_messages";
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
		case "entries":
			return client.getEntries();
		case "tree":
			return client.getTree();
		case "fork_messages":
			return client.getForkMessages();
		default:
			throw new Error("unknown test method");
	}
}

// Real private stdio child; no provider or network. Both clients run these exact bytes.
const child = await readFile(
	new URL("./rpc-client-child.mjs", import.meta.url),
	"utf8",
);

function readCanonicalSignalNames(): Record<string, string> {
	const groups = new Map<number, string[]>();
	for (const [name, number] of Object.entries(osConstants.signals)) {
		const aliases = groups.get(number) ?? [];
		aliases.push(name);
		groups.set(number, aliases);
	}
	const names: Record<string, string> = {};
	for (const [number, aliases] of groups) {
		let canonical = aliases[0];
		if (aliases.length > 1) {
			// Resolve aliases using a real private exit; disable core files only in that child.
			const child = spawnSync(
				"python3",
				[
					"-c",
					`import os,resource;resource.setrlimit(resource.RLIMIT_CORE,(0,0));os.kill(os.getpid(),${number})`,
				],
				{ timeout: 2000 },
			);
			if (!child.signal)
				throw new Error(
					`signal ${number}: ${child.error ?? child.stderr.toString()}`,
				);
			canonical = child.signal;
		}
		if (canonical === undefined) throw new Error(`empty aliases for ${number}`);
		names[String(number)] = canonical;
	}
	return names;
}

async function execute(api: Api, request: Request): Promise<unknown> {
	if (request.operation === "signal_names")
		return { ok: true, value: readCanonicalSignalNames() };
	const dir = await mkdtemp(join(tmpdir(), "rpc-client-oracle-"));
	const path = join(dir, "child.mjs");
	await writeFile(path, child);
	const client = new api.RpcClient({
		cliPath: request.cliPath ?? path,
		executablePath: request.executable,
		cwd: request.cwd,
		nodeArgs: request.nodeArgs,
		env: {
			PI_RPC_CLIENT_REPLY: JSON.stringify(request.reply),
			PI_RPC_STARTUP_SIGNAL: request.signal ?? "",
			PI_RPC_EXTRA_EVENTS: JSON.stringify(request.events ?? []),
			PI_RPC_WRITING: request.keepWriting ? "1" : "0",
			PI_RPC_DELAY_ACK: request.operation === "promise_lifetime" ? "1" : "0",
			PI_RPC_EXIT_ON_LINE:
				request.operation === "prompt_fail_lifetime" ? "1" : "0",
			PI_RPC_BLOCK_INPUT: request.closeInput
				? "close"
				: request.blockInput
					? "1"
					: "0",
			...request.envOverrides,
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
		if (request.operation === "startup_signal") {
			const starting = startOutcome(client);
			const child = Reflect.get(client, "process") as {
				exitCode: number | null;
				signalCode: string | null;
				once(event: "exit", listener: () => void): void;
			};
			if (child.exitCode === null && child.signalCode === null)
				await new Promise<void>((resolve) => child.once("exit", resolve));
			const start = await starting;
			const send = await client.getState().then(
				() => ({ ok: true }),
				(error: unknown) => {
					if (!(error instanceof Error)) throw error;
					return {
						ok: false,
						error: { name: error.name, message: error.message },
					};
				},
			);
			return { ok: true, value: { start, send } };
		}
		await client.start();
		if (
			request.operation === "promise_lifetime" ||
			request.operation === "prompt_fail_lifetime"
		) {
			const unhandled = new Promise((resolve) => {
				process.once("unhandledRejection", (error: unknown) => {
					if (!(error instanceof Error)) throw error;
					resolve({ name: error.name, message: error.message });
				});
			});
			const result = client
				.promptAndWait(
					"input",
					undefined,
					request.operation === "promise_lifetime" ? 10 : 200,
				)
				.then(
					(events) => ({ ok: true, events }),
					(error: unknown) => {
						if (!(error instanceof Error)) throw error;
						return {
							ok: false,
							error: { name: error.name, message: error.message },
						};
					},
				);
			return {
				ok: true,
				value: { unhandled: await unhandled, result: await result },
			};
		}
		if (request.operation === "pipe_error") {
			const inputClosed = new Promise<void>((resolve) => {
				const remove = client.onEvent((event) => {
					if (event.type === "stdin_closed") {
						remove();
						resolve();
					}
				});
			});
			const first = client.getState().then(
				() => null,
				(error) => (error instanceof Error ? error.message : String(error)),
			);
			await inputClosed;
			const second = client.getState().then(
				() => null,
				(error) => (error instanceof Error ? error.message : String(error)),
			);
			return { ok: true, value: await Promise.all([first, second]) };
		}
		if (request.operation === "blocked_write") {
			const results = await Promise.allSettled([
				client.prompt("x".repeat(2 * 1024 * 1024)),
				client.stop(),
			]);
			return {
				ok: true,
				value: results.map((result) =>
					result.status === "fulfilled"
						? null
						: result.reason instanceof Error
							? result.reason.message
							: String(result.reason),
				),
			};
		}
		if (request.operation === "stop_writing") {
			await client.stop();
			await new Promise((resolve) => setTimeout(resolve, 100));
			return { ok: true, value: { stderr: client.getStderr() } };
		}
		if (request.operation === "stop_fail") {
			const child = Reflect.get(client, "process") as {
				_handle: { kill(signal: number): number };
			};
			const handle = child._handle;
			const kill = handle.kill.bind(handle);
			let calls = 0;
			handle.kill = () => {
				calls++;
				return request.missingPid ? -3 : 0;
			};
			const results = await Promise.allSettled([client.stop(), client.stop()]);
			handle.kill = kill;
			const errors = results.map((result) =>
				result.status === "rejected" && result.reason instanceof Error
					? result.reason.message
					: null,
			);
			await client.stop();
			return { ok: true, value: { calls, errors } };
		}
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
		process.stdout.write(`${JSON.stringify(await execute(api, request))}\n`);
	}
}
await main();
