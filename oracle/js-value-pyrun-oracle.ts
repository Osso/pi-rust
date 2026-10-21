import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle((api, request) => {
	const formatResult = api.formatCanonicalPyrunEvalResult as (params: unknown, result: unknown) => unknown;
	const progressReporter = api.createPyrunProgressReporter as (
		onUpdate: (result: unknown) => void,
	) => (update: unknown) => void;
	const progress: unknown[] = [];
	progressReporter((result) => progress.push(result))(request.update);
	return { result: formatResult(request.params, request.result), progress };
}).catch((error: unknown) => {
	process.stderr.write(`Pyrun value oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
