import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Scheduler = { wait(goal: unknown, ctx: unknown, report: string, reason: string, displayId: string): Promise<void>; clearAll(): void; createReviewGuard(ctx: unknown): () => boolean };
runOracle(async (api, request) => {
	const events: unknown[] = [];
	const timers = new Map<number, () => void>();
	const oldSet = globalThis.setTimeout;
	const oldClear = globalThis.clearTimeout;
	let next = 0;
	globalThis.setTimeout = ((callback: () => void) => { timers.set(++next, callback); return next; }) as unknown as typeof setTimeout;
	globalThis.clearTimeout = ((id: number) => { timers.delete(id); }) as unknown as typeof clearTimeout;
	const ctx = { sessionManager: { getSessionId: () => "session" }, isIdle: () => true, hasPendingMessages: () => false };
	const scheduler = (api.createCompletionWaitScheduler as (options: unknown) => Scheduler)({
		pi: { async callTool() { events.push("list"); return { content: [], details: { activeCount: 0 } }; } },
		reviewGoal: async (input: { payload: unknown }) => { events.push(["review", input.payload]); return { decision: request.decision, evidenceCount: 3 }; },
		consumeReviewEvidence: (_ctx: unknown, _goal: unknown, count: number) => { events.push(["consume", count]); },
		isSameGoal: () => true,
		onComplete: () => { events.push("complete"); }, onContinue: (instructions: string, displayId: string) => { events.push(["continue", instructions, displayId]); },
		onStatus: (_ctx: unknown, message: string, reviewAt: string | undefined, displayId: string) => { events.push(["status", message, reviewAt !== undefined, displayId]); },
		onClearStatus: () => { events.push("clear"); }, onError: (error: Error) => { events.push(["error", error.message]); },
	});
	try {
		const guard = scheduler.createReviewGuard(ctx);
		events.push(["guard", guard()]);
		await scheduler.wait({ objective: "Ship it" }, ctx, "tests pass", "first wait", "initial");
		const timer = [...timers.values()][0];
		timer();
		for (let index = 0; index < 20; index++) await Promise.resolve();
		events.push(["guard", guard()]);
		return events;
	} finally { scheduler.clearAll(); globalThis.setTimeout = oldSet; globalThis.clearTimeout = oldClear; }
}).catch(error => { console.error(error); process.exitCode = 1; });
