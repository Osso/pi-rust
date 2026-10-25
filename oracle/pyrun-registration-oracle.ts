import { runOracle } from "./coding-agent-utils-oracle-support.ts";
runOracle(async (api, request) => {
	if (request.operation === "resume") {
		let selected: string | undefined;
		const dispatch = (api.createPyrunPiDispatcher as (pi: unknown, options: unknown) => (request: unknown, ctx: unknown, signal: undefined) => Promise<unknown>)({}, {});
		const result = await dispatch({ method: "sessions.resume", params: { name: "metadata-only" } }, { cwd: request.cwd, controlDbPath: request.controlDbPath, sessionManager: { getCwd: () => request.cwd, getSessionDir: () => request.sessionDir }, async switchSession(path: string) { selected = path; return { cancelled: false }; } }, undefined);
		return { result, selected };
	}
	if (request.operation === "render-empty") {
		type Component = {render(width: number): string[]};
		const tool = (api.createPyrunToolDefinition as (evaluate: unknown) => {renderCall(...args: unknown[]): Component; renderResult(...args: unknown[]): Component})(async () => ({}));
		const theme = { fg(_key: unknown, text: string) { return text; }, bold(text: string) { return text; } };
		return { call: tool.renderCall({code: "", session_id: ""}, theme, {}).render(80), result: tool.renderResult({content:[{type:"text",text:"\n\nvalue"}],details:{executed:""}}, {}, theme, {args:{code:""},isError:false}).render(80) };
	}
	if (request.operation === "definition") {
		const tool = (api.createPyrunToolDefinition as (evaluate: unknown, options: unknown) => {promptSnippet: string; promptGuidelines: string[]})(async () => ({}), request.options);
		return { promptSnippet: tool.promptSnippet, promptGuidelines: tool.promptGuidelines };
	}
	if (request.operation === "disposal") {
		process.env.PI_PYRUN_RUNNER_COMMAND = "/bin/true";
		const events: string[] = [];
		let shutdown!: () => Promise<void>;
		const pi = { registerTool() {}, on(name: string, callback: () => Promise<void>) { if (name === "session_shutdown") shutdown = callback; } };
		const handler = Object.assign(async () => undefined, { dispose() { events.push("disposed"); } });
		(api.default as (pi: unknown, options: unknown) => void)(pi, { piRequestHandlers: [handler] });
		await shutdown();
		return events;
	}
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
