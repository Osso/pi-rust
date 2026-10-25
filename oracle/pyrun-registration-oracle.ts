import { runOracle } from "./coding-agent-utils-oracle-support.ts";
runOracle((api, request) => {
	process.env.PI_PYRUN_RUNNER_COMMAND = request.command as string;
	process.env.PI_PYRUN_RUNNER_ARGS = "invalid JSON deliberately deferred until evaluation";
	let tool: { name: string; description: string; promptSnippet: string; promptGuidelines: string[] } | undefined;
	const pi = { on() {}, registerTool(value: typeof tool) { tool = value; } };
	(api.default as (pi: unknown) => void)(pi);
	return tool ? { name: tool.name, description: tool.description, promptSnippet: tool.promptSnippet, promptGuidelines: tool.promptGuidelines } : null;
}).catch(error => { console.error(error); process.exitCode = 1; });
