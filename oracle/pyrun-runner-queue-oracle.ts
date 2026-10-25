import { runOracle } from "./coding-agent-utils-oracle-support.ts";
type Runner = {
	evaluate(
		params: unknown,
		progress?: (update: unknown) => void,
	): Promise<unknown>;
	dispose(): void;
};
runOracle(async (api, request) => {
	if (request.operation === "resolved-options") {
		return (api.resolvePyrunRunnerOptions as (resolution: unknown) => unknown)({ env: {} });
	}
	const runner = new (
		api.PyrunRunnerClient as new (
			options: unknown,
		) => Runner
	)(request.options);
	try {
		const settled = await Promise.allSettled([
			runner.evaluate({ code: "first" }, () => {
				throw new Error("progress rejected");
			}),
			runner.evaluate({ code: "second" }),
		]);
		return settled.map((result) =>
			result.status === "fulfilled"
				? { result: result.value }
				: { error: (result.reason as Error).message },
		);
	} finally {
		runner.dispose();
	}
}).catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
