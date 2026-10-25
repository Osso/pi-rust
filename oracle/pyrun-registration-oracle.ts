import { runOracle } from "./coding-agent-utils-oracle-support.ts";
runOracle(async (api, request) => {
	if (request.operation === "dispatcher") {
		const dispatch = (api.createPyrunPiDispatcher as (pi: unknown, options: unknown) => (request: unknown, ctx: unknown, signal: undefined) => Promise<unknown>)({ getActiveTools: () => [] }, { piRequestHandlers: [async () => "handled"] });
		try { return { result: await dispatch({ method: request.method, params: null }, { noSupervisor: request.standalone }, undefined) }; }
		catch (error) { return { error: (error as Error).message }; }
	}
	process.env.PI_PYRUN_RUNNER_COMMAND = request.command as string;
	process.env.PI_PYRUN_RUNNER_ARGS = "invalid JSON deliberately deferred until evaluation";
	let tool: { name: string; description: string; promptSnippet: string; promptGuidelines: string[] } | undefined;
	const pi = { on() {}, registerTool(value: typeof tool) { tool = value; } };
	(api.default as (pi: unknown) => void)(pi);
	return tool ? { name: tool.name, description: tool.description, promptSnippet: tool.promptSnippet, promptGuidelines: tool.promptGuidelines } : null;
}).catch(error => { console.error(error); process.exitCode = 1; });
