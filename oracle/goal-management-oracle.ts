import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

type Tool = { execute(id: string, params: unknown, signal: undefined, update: undefined, ctx: unknown): Promise<{ content: unknown; details: unknown }> };
runOracle(async (api, request) => {
	const outputs: unknown[] = [];
	for (const value of requireCases(request)) {
		const input = value as { params: unknown; decision: unknown };
		let goal = JSON.stringify({ objective: "Ship it", branch: "main", createdAt: "2026-01-01T00:00:00.000Z", continuationTurns: 0 });
		let tool!: Tool;
		const notifications: unknown[] = [];
		const entries: unknown[] = [];
		const ctx = { cwd: "/tmp", isIdle: () => true, hasPendingMessages: () => false,
			sessionManager: { getSessionId: () => "goal-oracle", getSessionGoalJson: () => goal, setSessionGoalJson: (json: string) => { goal = json; }, clearSessionGoalJson: () => { goal = ""; }, getBranch: () => [] },
			ui: { setStatus() {}, requestRender() {}, notify(message: string, type: string) { notifications.push([message, type]); } } };
		const pi = { registerTool(value: Tool) { tool = value; }, registerCommand() {}, registerEntryRenderer() {}, registerMessageRenderer() {}, on() {}, appendEntry(_type: string, data: { message: string }) { entries.push(data.message); }, sendMessage() {}, sendUserMessage() {}, requestResumeContinuation() {}, async callTool() { return { content: [], details: { activeCount: 0 } }; } };
		(api.default as (pi: unknown, options: unknown) => void)(pi, { reviewGoal: async () => input.decision });
		const result = await tool.execute("call-1", input.params, undefined, undefined, ctx);
		outputs.push({ result, notifications, entries });
	}
	return outputs;
}).catch(error => { console.error(error); process.exitCode = 1; });
