import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	const invoke = api[request.operation] as (...args: unknown[]) => unknown;
	return Promise.all(requireCases(request).map(async (value) => {
		const args = value as unknown[];
		return (await invoke(...args)) ?? null;
	}));
}).catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
