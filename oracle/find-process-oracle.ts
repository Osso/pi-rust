import * as fs from "node:fs/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createFindToolDefinition(cwd: string): {
		execute(id: string, args: unknown): Promise<unknown>;
	};
};
type Input = { pattern?: string; limit?: number; number?: string };

void runOracle(async (module, request) => {
	const cwd = String(request.cwd);
	process.chdir(cwd);
	const tool = (module as Api).createFindToolDefinition(cwd);
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		process.env.PI_FAKE_FD_CASE = JSON.stringify(value);
		await fs.rm(String(request.argvFile), { force: true });
		const limit = input.number === undefined ? input.limit : Number(input.number);
		let result: unknown;
		try {
			result = { ok: true, value: await tool.execute("fd-process", { pattern: input.pattern ?? "*.x", path: "search", limit }) };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		let argv: unknown = null;
		try {
			argv = JSON.parse(await fs.readFile(String(request.argvFile), "utf8"));
		} catch (error) {
			if ((error as { code?: string }).code !== "ENOENT") throw error;
		}
		output.push({ result, argv });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find process oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
