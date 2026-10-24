import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createFindToolDefinition(cwd: string, options: unknown): {
		execute(id: string, args: unknown): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	process.chdir(String(request.processCwd));
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { paths: string[] };
		const tool = (module as Api).createFindToolDefinition("/repo", {
			operations: { exists: () => true, glob: () => input.paths },
		});
		output.push(await tool.execute("find-paths", { pattern: "*", path: "list" }));
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find path oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
