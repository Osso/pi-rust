import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { context?: number; limit?: number; content?: string; readError?: boolean };
type Api = { createGrepToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown): Promise<unknown> } };

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		process.env.PI_FAKE_RG_PROTOCOL = JSON.stringify(value);
		const calls: unknown[] = [];
		const tool = (module as Api).createGrepToolDefinition("/repo", {
			operations: {
				isDirectory(path: string) { calls.push(["isDirectory", path]); return true; },
				readFile(path: string) {
					calls.push(["readFile", path]);
					if (input.readError) throw new Error("scripted read failure");
					return input.content ?? "one\ntwo\nthree\nfour\nfive";
				},
			},
		});
		let result: unknown;
		try {
			result = { ok: true, value: await tool.execute("grep-protocol", { pattern: "x", path: "search", context: input.context, limit: input.limit }) };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		output.push({ result, calls });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep protocol oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
