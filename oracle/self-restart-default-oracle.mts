import { existsSync, readFileSync, writeFileSync } from "node:fs";
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
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as {
		root: string;
		operation: "runtime-default" | "spawn-default";
	};
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
