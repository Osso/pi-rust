import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Scheduler = { waitForAgentsOrScheduleReview(ctx: unknown, goal: unknown, turn: unknown[], callbacks: unknown): Promise<void>; clearAll(): void };

runOracle(async (api, request) => {
	if (request.operation === "report-error") {
		let reject!: (error: Error) => void;
		const wake = new Promise<void>((_resolve, onReject) => { reject = onReject; });
		const unhandled = new Promise<string>(resolve => {
			process.once("unhandledRejection", error => resolve((error as Error).message));
		});
		const scheduler = (api.createGoalScheduler as (options: unknown) => Scheduler)({
			pi: { async callTool(name: string) { if (name === "wait_agent") await wake; return { content: [], details: { activeCount: 1 } }; } },
			reviewGoal: async () => ({}), applyDecision: async () => {}, isSameRunningGoal: () => true,
			reportError: () => { throw new Error("status append failed"); },
		});
		try {
			await scheduler.waitForAgentsOrScheduleReview({ sessionManager: { getSessionId: () => "session" }, isIdle: () => true, hasPendingMessages: () => false }, {}, [], { onAgentWait() {}, onAgentWake() {}, onReviewScheduled() {} });
			reject(new Error("wait failed"));
			return await unhandled;
		} finally { scheduler.clearAll(); }
	}
	const events: string[] = [];
	const timers = new Map<number, { tick: () => void; at: number }>();
	let now = 1_700_000_000_000;
	let nextTimer = 0;
	const oldNow = Date.now;
	const oldSet = globalThis.setTimeout;
	const oldClear = globalThis.clearTimeout;
	Date.now = () => now;
	globalThis.setTimeout = ((tick: () => void, delay: number) => {
		const id = ++nextTimer;
		timers.set(id, { tick, at: now + delay });
		return id;
	}) as unknown as typeof setTimeout;
	globalThis.clearTimeout = ((id: number) => { timers.delete(id); }) as unknown as typeof clearTimeout;
	let release!: () => void;
	const waiting = new Promise<void>(resolve => { release = resolve; });
	const ctx = { sessionManager: { getSessionId: () => "session" }, hasPendingMessages: () => false, isIdle: () => true };
	const scheduler = (api.createGoalScheduler as (options: unknown) => Scheduler)({
		pi: { async callTool(name: string) {
			events.push(name);
			if (name === "wait_agent") await waiting;
			return { content: [], details: { activeCount: 1 } };
		} },
		reviewGoal: async () => { events.push("review"); return {}; },
		applyDecision: async () => { events.push("apply"); },
		isSameRunningGoal: () => true,
		reportError: (error: Error) => { events.push(`error:${error.message}`); },
	});
	try {
		await scheduler.waitForAgentsOrScheduleReview(ctx, {}, [], {
			onAgentWait: () => { events.push("waiting"); }, onAgentWake: () => { events.push("wake"); },
			onReviewScheduled: () => { events.push("scheduled"); },
		});
		const timer = [...timers.values()][0];
		if (!timer || timer.at !== now + 1_500_000) throw new Error("missing twenty-five-minute agent-wait review timer");
		now = timer.at;
		timer.tick();
		for (let index = 0; index < 10; index++) await Promise.resolve();
		release();
		for (let index = 0; index < 10; index++) await Promise.resolve();
		return events;
	} finally {
		scheduler.clearAll();
		Date.now = oldNow;
		globalThis.setTimeout = oldSet;
		globalThis.clearTimeout = oldClear;
	}
}).catch(error => { console.error(error); process.exitCode = 1; });
