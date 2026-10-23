import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createLsToolDefinition(cwd: string): {
		execute(id: string, args: unknown): Promise<unknown>;
	};
};

type IoCase = {
	stdout: string;
	stderr: string;
	code: number;
	cwd?: string;
	path?: string;
	limit?: number;
};

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as IoCase;
		process.env.PI_FAKE_RTK_IO_CASE = JSON.stringify(input);
		try {
			const tool = (module as Api).createLsToolDefinition(input.cwd ?? String(request.cwd));
			const result = await tool.execute("rtk-io", { path: input.path ?? "list", limit: input.limit });
			output.push({ ok: true, value: result });
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			output.push({ ok: false, error: error.message });
		}
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`rtk io oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
