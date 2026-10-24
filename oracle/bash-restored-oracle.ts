import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";
type Input = { agent: unknown; timeout?: number };
type Api = { createBashToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown): Promise<unknown> } };
void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		const lifecycle = { findBashJobByToolCallId: () => input.agent };
		const tool = (module as Api).createBashToolDefinition(String(request.cwd), {
			backgroundJobs: { lifecycle, store: {} }, detach: { signal: new AbortController().signal },
		});
		try { output.push({ ok: true, value: await tool.execute("restore", { command: "must not launch", timeout: input.timeout }) }); }
		catch (error) { if (!(error instanceof Error)) throw error; output.push({ ok: false, error: error.message }); }
	}
	return output;
}).catch((error: unknown) => { process.stderr.write(`bash restored oracle failed: ${String(error)}\n`); process.exitCode = 1; });
