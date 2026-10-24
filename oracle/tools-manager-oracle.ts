import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Tool = "fd" | "rg";
type Api = {
	getToolPath(tool: Tool): string | null;
	ensureTool(tool: Tool, silent?: boolean): Promise<string | undefined>;
};

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { tool: Tool; path: string; silent: boolean };
		const path = process.env.PATH;
		const log = console.log;
		const messages: string[] = [];
		process.env.PATH = input.path;
		console.log = (...args: unknown[]) => messages.push(args.map(String).join(" "));
		try {
			const api = module as Api;
			const found = api.getToolPath(input.tool);
			const ensured = await api.ensureTool(input.tool, input.silent);
			output.push({ found, ensured: ensured ?? null, messages });
		} finally {
			if (path === undefined) delete process.env.PATH;
			else process.env.PATH = path;
			console.log = log;
		}
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`tools manager oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
