import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { path: string; offset?: number; limit?: number };
type ReadToolApi = {
	createReadTool(cwd: string): {
		execute(
			toolCallId: string,
			params: Case,
		): Promise<{ content: unknown[]; details?: unknown }>;
	};
};

void runOracle(async (module, request) => {
	const api = module as unknown as ReadToolApi;
	const cwd = String(request.cwd);
	const tool = api.createReadTool(cwd);
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			const result = await tool.execute("read-differential", input);
			results.push({ ok: true, value: result });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent read-tool oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
