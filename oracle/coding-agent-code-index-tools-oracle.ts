import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { kind: "outline" | "symbol" | "references"; params: Record<string, unknown> };
type Tool = { execute(id: string, params: Record<string, unknown>): Promise<unknown> };
type Api = {
	createOutlineTool(cwd: string): Tool;
	createSymbolTool(cwd: string): Tool;
	createReferencesTool(cwd: string): Tool;
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const cwd = String(request.cwd);
	const tools = {
		outline: api.createOutlineTool(cwd),
		symbol: api.createSymbolTool(cwd),
		references: api.createReferencesTool(cwd),
	};
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			results.push({ ok: true, value: await tools[input.kind].execute("code-index-differential", input.params) });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent code-index-tools oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
