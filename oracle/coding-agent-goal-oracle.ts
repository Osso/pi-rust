import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("goal oracle requires a source module path");
	const sourceDirectory = dirname(sourceModulePath);

	async function load<T>(name: string): Promise<T> {
		return (await import(pathToFileURL(join(sourceDirectory, name)).href)) as T;
	}

	const parsing = await load<{
		parseGoal(value: unknown): unknown;
		parseGoalJson(value: string): unknown;
	}>("goal-parsing.ts");
	const objective = await load<{ deduplicateCurrentGoalScope(a: string, b: string): string }>("goal-objective.ts");
	const presentation = await load<{
		goalFooterStatus(goal: unknown): string;
		goalStartupMessage(goal: unknown): string;
		goalViewMessage(goal: unknown): string;
		goalSystemBlock(goal: unknown): string;
	}>("goal-presentation.ts");
	const turn = await load<{
		didLastAssistantAbort(event: unknown): boolean;
		didLastAssistantReturnEmpty(event: unknown): boolean;
		findLastAssistantMessage(event: unknown): { stopReason?: string } | undefined;
	}>("goal-turn.ts");

	type Case =
		| { kind: "parse-goal"; value: unknown }
		| { kind: "parse-goal-json"; value: string }
		| { kind: "dedupe"; reviewed: string; current: string }
		| { kind: "present"; goal: unknown }
		| { kind: "turn"; event: unknown };

	await runOracle((api, request) => {
		if (request.operation === "parse-args") {
			return requireCases(request).map((value) =>
				(api.parseGoalArgs as (args: string) => unknown)(String(value)),
			);
		}
		return requireCases(request).map((value) => {
			const input = value as Case;
			switch (input.kind) {
				case "parse-goal":
					return parsing.parseGoal(input.value);
				case "parse-goal-json":
					return parsing.parseGoalJson(input.value);
				case "dedupe":
					return objective.deduplicateCurrentGoalScope(input.reviewed, input.current);
				case "present":
					return {
						footer: presentation.goalFooterStatus(input.goal),
						startup: presentation.goalStartupMessage(input.goal),
						view: presentation.goalViewMessage(input.goal),
						block: presentation.goalSystemBlock(input.goal),
					};
				case "turn":
					return {
						aborted: turn.didLastAssistantAbort(input.event),
						empty: turn.didLastAssistantReturnEmpty(input.event),
						stopReason: turn.findLastAssistantMessage(input.event)?.stopReason,
					};
			}
		});
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`coding-agent goal oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
