import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createFindToolDefinition(cwd: string, options: unknown): {
		execute(id: string, args: unknown): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { limit?: number; number?: string };
		const calls: unknown[] = [];
		const operations = {
			exists(path: string) {
				calls.push(["exists", path]);
				return true;
			},
			glob(pattern: string, cwd: string, options: { ignore: string[]; limit: number }) {
				calls.push(["glob", pattern, cwd, options.ignore, String(options.limit)]);
				return ["/repo/list/alpha", "/repo/list/beta", "/repo/list/gamma"];
			},
		};
		const tool = (module as Api).createFindToolDefinition("/repo", { operations });
		const limit = input.number === undefined ? input.limit : Number(input.number);
		const result = await tool.execute("find-numbers", { pattern: "*.ts", path: "list", limit });
		output.push({ calls, result });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find numeric oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
