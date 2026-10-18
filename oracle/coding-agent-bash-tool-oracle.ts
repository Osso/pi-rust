import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { command: string; timeout?: number; prefix?: string };
type Api = {
	createBashTool(cwd: string, options?: { commandPrefix?: string }): {
		execute(id: string, params: { command: string; timeout?: number }): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const cwd = String(request.cwd);
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			const tool = api.createBashTool(cwd, input.prefix ? { commandPrefix: input.prefix } : undefined);
			const result = await tool.execute("bash-differential", {
				command: input.command,
				...(input.timeout === undefined ? {} : { timeout: input.timeout }),
			});
			results.push({ ok: true, value: result });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent bash-tool oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
