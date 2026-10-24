import { setImmediate } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createFindToolDefinition(cwd: string, options: unknown): {
		execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { phase: string; reject?: boolean; preabort?: boolean };
		const events: string[] = [];
		const started = Promise.withResolvers<void>();
		const gate = Promise.withResolvers<void>();
		const done = Promise.withResolvers<void>();
		const operation = async (phase: string) => {
			events.push(`${phase}:start`);
			if (phase === input.phase) {
				started.resolve();
				await gate.promise;
				if (input.reject) {
					events.push(`${phase}:reject`);
					done.resolve();
					throw new Error("operation rejected");
				}
			}
			events.push(`${phase}:end`);
			if (phase === input.phase) done.resolve();
		};
		const tool = (module as Api).createFindToolDefinition("/repo", {
			operations: {
				exists: async () => { await operation("exists"); return true; },
				glob: async () => { await operation("glob"); return ["/repo/list/a"]; },
			},
		});
		const controller = new AbortController();
		if (input.preabort) controller.abort({ ignored: "reason" });
		const result = tool.execute("find-abort", { pattern: "*", path: "list" }, controller.signal)
			.then(value => ({ ok: true, value }), error => ({ ok: false, error: error.message }));
		if (input.preabort) {
			output.push({ result: await result, events, continued: true });
			continue;
		}
		await started.promise;
		controller.abort({ ignored: "reason" });
		const observed = await result;
		const before = [...events];
		gate.resolve();
		const continued = await Promise.race([done.promise.then(() => true), new Promise<boolean>(resolve => setTimeout(() => resolve(false), 300))]);
		await setImmediate();
		output.push({ result: observed, before, events, continued });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find abort oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
