import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; source: string };

void runOracle((api, request) => {
	const parseGitUrl = api.parseGitUrl as (source: string) => unknown;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			return { name: input.name, value: parseGitUrl(input.source) ?? null };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`git oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
