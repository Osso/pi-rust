import { setImmediate } from "node:timers/promises";
import { createPromiseGate } from "./promise-gate.ts";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createGrepToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown> } };

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { phase: string; preabort?: boolean };
		const events: string[] = [];
		const started = createPromiseGate();
		const release = createPromiseGate();
		const operation = async (phase: string) => {
			events.push(`${phase}:start`);
			if (phase === input.phase) { started.resolve(); await release.promise; }
			events.push(`${phase}:end`);
		};
		const tool = (module as Api).createGrepToolDefinition("/repo", {
			operations: {
				isDirectory: async () => { await operation("isDirectory"); return true; },
				readFile: async () => { await operation("readFile"); return "one\ntwo\nthree"; },
			},
		});
		const controller = new AbortController();
		if (input.preabort) controller.abort();
		let settled = false;
		const result = tool.execute("grep-window", { pattern: "x", path: "search", context: input.phase === "readFile" ? 1 : 0 }, controller.signal)
			.then(value => ({ ok: true, value }), error => ({ ok: false, error: error.message }))
			.then(value => { settled = true; return value; });
		if (input.preabort) { output.push({ result: await result, events }); continue; }
		await started.promise;
		controller.abort({ ignored: "reason" });
		await setImmediate();
		const settledBeforeRelease = settled;
		release.resolve();
		output.push({ result: await result, events, settledBeforeRelease });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep abort window oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
