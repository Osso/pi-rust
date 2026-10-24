import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createGrepToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown): Promise<unknown> } };

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as { slot: "limit" | "context"; value?: number; number?: string; absent?: boolean };
		const calls: unknown[] = [];
		let reads = 0;
		const tool = (module as Api).createGrepToolDefinition("/repo", {
			operations: {
				isDirectory(path: string) { calls.push(["isDirectory", path]); return true; },
				readFile(path: string) {
					calls.push(["readFile", path]);
					reads++;
					return `one\ntwo\nthird-version-${reads}\nfour\nfive`;
				},
			},
		});
		const args: Record<string, unknown> = { pattern: "x", path: "search" };
		if (!input.absent) args[input.slot] = input.number === undefined ? input.value : Number(input.number);
		output.push({ result: await tool.execute("grep-number", args), calls });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep numeric oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
