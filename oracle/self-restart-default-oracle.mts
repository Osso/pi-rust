import {
	chmodSync,
	copyFileSync,
	existsSync,
	readFileSync,
	writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
const {
	spawnSelfRestart,
	ENV_SELF_RESTART_REQUEST,
	ENV_SELF_RESTART_SESSION,
	ENV_SELF_RESTART_PROMPT,
	ENV_SELF_RESTART_OLD_PID,
} = await import(pathToFileURL(`${source}/core/self-restart.ts`).href);
interface Proof {
	proof: "runtime-default" | "spawn-default";
	before: string;
	after: string;
}
if (process.env[ENV_SELF_RESTART_REQUEST] === "1") {
	const handoff = JSON.parse(
		process.env[ENV_SELF_RESTART_PROMPT] ?? "null",
	) as Proof | null;
	if (
		handoff?.proof === "runtime-default" ||
		handoff?.proof === "spawn-default"
	) {
		const before = JSON.parse(readFileSync(handoff.before, "utf8")) as {
			pid: number;
			cwd: string;
		};
		const session = process.env[ENV_SELF_RESTART_SESSION];
		writeFileSync(
			handoff.after,
			JSON.stringify({
				pid: process.pid,
				samePid: process.pid === before.pid,
				oldPidMatches:
					process.env[ENV_SELF_RESTART_OLD_PID] === String(before.pid),
				cwdRetained: process.cwd() === before.cwd,
				sessionExists: session !== undefined && existsSync(session),
				marker: process.env[ENV_SELF_RESTART_REQUEST],
			}),
		);
		process.exit(0);
	}
}
async function proveUnobservedSpawnError(
	root: string,
	line: string,
): Promise<never> {
	if (process.env.PI_PRIVATE_RESTART_IMAGE !== "1") {
		const image = join(root, "node-copy");
		copyFileSync(process.execPath, image);
		const child = spawnSync(
			image,
			[...process.execArgv, ...process.argv.slice(1)],
			{
				input: line + "\n",
				env: { ...process.env, PI_PRIVATE_RESTART_IMAGE: "1" },
				encoding: "utf8",
			},
		);
		writeFileSync(
			join(root, "outcome.json"),
			JSON.stringify({
				code: child.status,
				signal: child.signal,
				stderr: child.stderr,
			}),
		);
		process.exit(0);
	}
	writeFileSync(
		join(root, "image.json"),
		JSON.stringify({
			file: process.execPath,
			args: [...process.execArgv, ...process.argv.slice(1)],
		}),
	);
	process.on("uncaughtExceptionMonitor", (error) => {
		const native = error as Error & {
			code?: string;
			errno?: number;
			syscall?: string;
			path?: string;
			spawnargs?: string[];
		};
		writeFileSync(
			join(root, "error.json"),
			JSON.stringify({
				error: {
					name: native.name,
					message: native.message,
					code: native.code,
					errno: native.errno,
					syscall: native.syscall,
					path: native.path,
					spawnargs: native.spawnargs,
				},
				string: String(native),
			}),
		);
	});
	// Only this private copied interpreter loses its execute bit.
	chmodSync(process.execPath, 0o600);
	const code = await spawnSelfRestart(
		{ sessionFile: join(root, "session.jsonl") },
		{ waitForExit: false },
	);
	writeFileSync(join(root, "returned.json"), JSON.stringify({ code }));
	await new Promise((resolve) => setTimeout(resolve, 1000));
	throw new Error("deferred default-spawn error was not uncaught");
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as {
		root: string;
		operation: "runtime-default" | "spawn-default" | "spawn-default-failure";
	};
	if (request.operation === "spawn-default-failure") {
		await proveUnobservedSpawnError(request.root, line);
		continue;
	}
	const proof: Proof = {
		proof: request.operation,
		before: join(request.root, "before.json"),
		after: join(request.root, "after.json"),
	};
	const manager = SessionManager.create(
		request.root,
		join(request.root, "sessions"),
	);
	manager.setMetadataControlDbPath(join(request.root, "control.db"));
	manager.persistForRecovery();
	writeFileSync(
		proof.before,
		JSON.stringify({ pid: process.pid, cwd: process.cwd() }),
	);
	if (request.operation === "spawn-default") {
		const code = await spawnSelfRestart({
			sessionFile: manager.getSessionFile(),
			prompt: JSON.stringify(proof),
		});
		if (code !== 0) throw new Error(`source default spawn exited ${code}`);
		process.exit(0);
	}
	const session = {
		sessionManager: manager,
		sessionFile: manager.getSessionFile(),
		model: { provider: "faux", id: "faux-1" },
		thinkingLevel: "off",
		setWorkingDirectoryRecoveryHandlers: () => {},
		extensionRunner: {
			setRelocateHandler: () => {},
			setToolResultRelocationHandlers: () => {},
			hasHandlers: () => false,
		},
		detachAllRunningTools: () => {},
		dispose: () => {},
	};
	const runtime = new AgentSessionRuntime(
		session,
		{ cwd: request.root, agentDir: request.root },
		async () => {
			throw new Error("unexpected runtime factory");
		},
	);
	await runtime.restart({ notice: JSON.stringify(proof), process: true });
	throw new Error("source default process restarter returned");
}
