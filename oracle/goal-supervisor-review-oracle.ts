import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { ctx: unknown; kind: string; payload: Record<string, unknown> };
type Review = (input: Input) => Promise<Record<string, unknown>>;
type Append = (ctx: unknown, message: string, reviewAt?: string, displayId?: string) => void;

runOracle(async (api, request) => {
	const wrap = api.withSupervisorReviewStatus as (append: Append, review: Review) => Review;
	return await Promise.all(requireCases(request).map(async value => {
		const item = value as { result?: Record<string, unknown>; error?: string };
		const statuses: unknown[] = [];
		let id: string | undefined;
		const review = wrap((_ctx, message, reviewAt, displayId) => {
			if (!id) id = displayId;
			statuses.push({ message, reviewAt, displayId: displayId === id ? "display-one" : "invalid-id" });
		}, async () => {
			if (item.error !== undefined) throw new Error(item.error);
			return item.result!;
		});
		const result = await review({ ctx: {}, kind: "goal_idle_review", payload: {} });
		return { statuses, result: { ...result, displayId: result.displayId === id ? "display-one" : "invalid-id" } };
	}));
}).catch(error => { console.error(error); process.exitCode = 1; });
