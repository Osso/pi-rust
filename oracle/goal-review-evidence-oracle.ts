import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Step = { op: string; event?: unknown; goal?: unknown; payload?: Record<string, unknown>; count?: number };
type Input = { ctx: unknown; payload: Record<string, unknown>; kind: string };
type Reviewed = { decision: unknown; evidenceCount: number };
type Controller = { appendInput(event: unknown, ctx: unknown): void; appendToolResult(event: unknown, ctx: unknown): void; consume(ctx: unknown, goal: unknown, count: number): void; review(input: Input): Promise<Reviewed> };
type Store = { loadActiveGoal(ctx: unknown): unknown; saveGoal(ctx: unknown, goal: unknown): void };
type Create = (store: Store, review: (input: Input) => Promise<unknown>) => Controller;

async function runCase(create: Create, steps: Step[]): Promise<unknown> {
	let goal: unknown = null;
	let reviewedGoal: unknown = null;
	let count = 0;
	const payloads: unknown[] = [];
	const snapshots: unknown[] = [];
	const ctx = { sessionManager: { getSessionId: () => "session-one" } };
	const controller = create({ loadActiveGoal: () => goal, saveGoal(_ctx, value) { goal = value; } }, async input => {
		payloads.push(input.payload);
		return { kind: "error", reason: "fixture" };
	});
	for (const step of steps) {
		let review: Reviewed | null = null;
		if (step.op === "goal") goal = step.goal ?? null;
		if (step.op === "input") controller.appendInput(step.event, ctx);
		if (step.op === "tool") controller.appendToolResult(step.event, ctx);
		if (step.op === "review") {
			reviewedGoal = goal;
			review = await controller.review({ ctx, kind: "goal_idle_review", payload: step.payload ?? {} });
			count = review.evidenceCount;
		}
		if (step.op === "consume") controller.consume(ctx, reviewedGoal, step.count ?? count);
		snapshots.push({ goal, review, payloads: [...payloads] });
	}
	return snapshots;
}

runOracle(async (api, request) => {
	const create = api.createGoalReviewEvidenceController as Create;
	return await Promise.all(requireCases(request).map(value => runCase(create, value as Step[])));
}).catch(error => { console.error(error); process.exitCode = 1; });
