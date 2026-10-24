import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type EndTurnApi = {
	createEndTurnToolDefinition(): {
		execute(
			toolCallId: string,
			params: { reason: string },
			signal?: AbortSignal,
			onUpdate?: (value: unknown) => void,
		): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const tool = (module as unknown as EndTurnApi).createEndTurnToolDefinition();
	if (request.operation === "abort-independent") {
		const updates: unknown[] = [];
		const result = await tool.execute(
			"unused",
			{ reason: " finished " },
			AbortSignal.abort("cancelled"),
			(value) => updates.push(value),
		);
		return { result, updates };
	}
	const results = [];
	for (const params of requireCases(request)) {
		results.push(
			await tool.execute("end-turn-differential", params as { reason: string }),
		);
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`end-turn oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
