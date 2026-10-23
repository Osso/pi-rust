import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle((api, request) => {
	const check = api.isExecutableAvailable as (command: string, options?: unknown) => boolean;
	return requireCases(request).map((value) => {
		const input = value as { command: string; options?: unknown };
		return check(input.command, input.options);
	});
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
