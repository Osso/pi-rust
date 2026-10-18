import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; cwd: string };

void runOracle((api, request) => {
	const formatTerminalCurrentDirectorySequence = api.formatTerminalCurrentDirectorySequence as (
		cwd: string,
	) => string;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			return { name: input.name, value: formatTerminalCurrentDirectorySequence(input.cwd) };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`terminal-current-directory oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
