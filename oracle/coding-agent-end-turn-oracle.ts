import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type EndTurnApi = {
	createEndTurnToolDefinition(): {
		execute(toolCallId: string, params: { reason: string }): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const tool = (module as unknown as EndTurnApi).createEndTurnToolDefinition();
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
