import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = {
	pattern: string;
	path?: string;
	glob?: string;
	ignoreCase?: boolean;
	literal?: boolean;
	context?: number;
	limit?: number;
};
type Api = {
	createGrepTool(cwd: string): { execute(id: string, params: Case): Promise<unknown> };
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const cwd = String(request.cwd);
	const tool = api.createGrepTool(cwd);
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			results.push({ ok: true, value: await tool.execute("grep-differential", input) });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent grep-tool oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
