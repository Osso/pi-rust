import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
async function main() {
	const root = process.argv[2];
	const { registerQuestionRelayTools } = await import(
		pathToFileURL(
			join(root, "extensions/agents-core/src/question-relay-tools.ts"),
		).href
	);
	const { MultiAgentStore } = await import(
		pathToFileURL(join(root, "src/core/multi-agent-store.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const store = new MultiAgentStore();
		for (const agent of request.agents ?? [])
			store.publishLifecycleCoordinatorSnapshot(agent);
		const tools = new Map<string, any>();
		const sent: unknown[] = [];
		registerQuestionRelayTools(
			{ registerTool: (tool: any) => tools.set(tool.name, tool) },
			store,
			{
				contactParent: (params: unknown) => {
					sent.push(params);
					return {
						content: [],
						details: { message: { id: "queued", status: "pending" } },
					};
				},
			},
		);
		const signal = new AbortController();
		if (request.cancelled) signal.abort();
		const ctx = {
			multiAgentAgentId: request.agentId,
			multiAgentRequiresAgentId: request.requiresId,
			controlDbPath: request.db,
			sessionManager: { getSessionId: () => "session-child" },
		};
		try {
			const result = await tools
				.get(request.tool ?? "ask_questions")
				.execute(request.id, request.params, signal.signal, undefined, ctx);
			console.log(
				JSON.stringify({
					result: {
						content: result.content,
						details: result.details,
						isError: result.isError ?? null,
					},
					sent,
				}),
			);
		} catch (error) {
			console.log(
				JSON.stringify({
					error: error instanceof Error ? error.message : String(error),
					sent,
				}),
			);
		}
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
