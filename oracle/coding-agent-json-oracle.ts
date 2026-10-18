import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; input: string };

void runOracle((api, request) => {
	const stripJsonComments = api.stripJsonComments as (value: string) => string;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			return { name: input.name, value: stripJsonComments(input.input) };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`json oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
