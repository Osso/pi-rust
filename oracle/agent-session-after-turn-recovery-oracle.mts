import { rmdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSession } = await import(
	pathToFileURL(`${root}/core/agent-session.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${root}/core/agent-session-runtime.ts`).href
);
const { ExtensionRunner } = await import(
	pathToFileURL(`${root}/core/extensions/runner.ts`).href
);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			cwd: string;
			selected: string;
			prepared: boolean;
		};
		let target: string | undefined;
		const runner = Object.assign(Object.create(ExtensionRunner.prototype), {
			isToolResultRelocationPrepared: false,
		});
		const agent: {
			shouldStopAfterTurn?: (context: object) => Promise<boolean>;
		} = {};
		const session = Object.create(AgentSession.prototype);
		Object.defineProperty(session, "agent", { value: agent });
		session._extensionRunner = runner;
		session._installAgentTurnStop();
		const runtime = Object.create(AgentSessionRuntime.prototype);
		runtime._session = session;
		runtime._services = { cwd: request.cwd };
		runtime.prepareToolResultRelocation = async (cwd: string) => {
			target = cwd;
		};
		runtime.bindSessionRuntimeActions();
		if (request.prepared)
			await runner.prepareWorkingDirectoryRelocation(request.selected);
		rmdirSync(request.cwd);
		if (!agent.shouldStopAfterTurn) throw new Error("frozen stop hook missing");
		const stop = await agent.shouldStopAfterTurn({});
		console.log(
			JSON.stringify({
				ok: true,
				stop,
				prepared: runner.hasPreparedToolResultRelocation(),
				target,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
