import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

runOracle(async (api, request) => {
	const { createGoalScheduler } = await import(
		pathToFileURL(join(dirname(process.argv[2]), "goal-scheduling.ts")).href
	);
	const variant = request.variant as string;
	const events: string[] = [];
	let pending = variant === "defer-review";
	let idle = false;
	const timers = new Map<number, () => void>();
	let next = 0;
	const oldSet = globalThis.setTimeout;
	const oldClear = globalThis.clearTimeout;
	globalThis.setTimeout = ((callback: () => void) => {
		timers.set(++next, callback);
		return next;
	}) as unknown as typeof setTimeout;
	globalThis.clearTimeout = ((id: number) => {
		timers.delete(id);
	}) as unknown as typeof clearTimeout;
	const ctx = {
		sessionManager: { getSessionId: () => "session" },
		isIdle: () => idle,
		hasPendingMessages: () => pending,
	};
	const applyDecision = async () => {
		events.push("apply");
	};
	const reviewed = {
		decision: { kind: "complete", reason: "ok" },
		evidenceCount: 0,
	};
	const scheduler = createGoalScheduler({
		pi: {},
		reviewGoal: async () => {
			events.push("review");
			return reviewed;
		},
		applyDecision,
		isSameRunningGoal: () => true,
		reportError: (error: Error) => events.push(error.message),
	});
	try {
		await (api.runScheduledGoalAgentEnd as (options: unknown) => Promise<void>)(
			{
				event: { messages: [] },
				ctx,
				scheduler,
				emptyResponseScheduler: {},
				applyDecision,
				selectGoal: () =>
					variant === "none" ? null : { objective: "Ship it" },
				isSameGoal: () => variant !== "changed",
				reviewGoal: async () => {
					events.push("review");
					if (variant === "cancel") scheduler.clearSession("session");
					if (variant === "defer-decision") pending = true;
					return reviewed;
				},
			},
		);
		for (let index = 0; index < 2; index++) {
			if (index === 1) {
				idle = true;
				pending = false;
			}
			const first = [...timers.entries()][0];
			if (first) {
				timers.delete(first[0]);
				first[1]();
			}
			for (let turn = 0; turn < 10; turn++) await Promise.resolve();
		}
		return events;
	} finally {
		scheduler.clearAll();
		globalThis.setTimeout = oldSet;
		globalThis.clearTimeout = oldClear;
	}
}).catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
