import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { goal: unknown; pending: boolean; stopReason: string; text: string; noAssistant?: boolean };
type Options = {
	event: unknown;
	ctx: { hasPendingMessages(): boolean; sessionManager: { getSessionId(): string } };
	selectGoal(): unknown;
	clearRetry(sessionId: string): void;
	scheduleRetry(ctx: unknown, goal: unknown): void;
	scheduleErrorStatus(ctx: unknown, message: string): void;
	reportSkipped(message: string): void;
};

runOracle((api, request) => {
	const select = api.selectGoalForIdleReview as (options: Options) => unknown;
	return requireCases(request).map(value => {
		const input = value as Input;
		const calls: unknown[] = [];
		const result = select({
			event: { messages: input.noAssistant ? [] : [{ role: "assistant", stopReason: input.stopReason, content: [{ type: "text", text: input.text }] }] },
			ctx: { hasPendingMessages() { calls.push(["pending"]); return input.pending; }, sessionManager: { getSessionId: () => "session" } },
			selectGoal() { calls.push(["select"]); return input.goal; },
			clearRetry() { calls.push(["clear"]); },
			scheduleRetry(_ctx, goal) { calls.push(["retry", goal]); },
			scheduleErrorStatus(_ctx, message) { calls.push(["error", message]); },
			reportSkipped(message) { calls.push(["report", message]); },
		});
		return { result, calls };
	});
}).catch(error => { console.error(error); process.exitCode = 1; });
