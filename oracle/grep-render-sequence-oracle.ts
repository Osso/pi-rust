import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Step = {
	kind: "call" | "result";
	args?: Record<string, unknown>;
	result?: { content: unknown[]; details?: Record<string, unknown> };
	number?: string;
	expanded?: boolean;
	showImages?: boolean;
	isError?: boolean;
	wrongComponent?: boolean;
};
type Definition = {
	renderCall(args: unknown, theme: unknown, context: unknown): Component;
	renderResult(result: unknown, options: unknown, theme: unknown, context: unknown): Component;
};
type Api = { createGrepToolDefinition(cwd: string): Definition };
const themePromise = (async () => {
	const source = process.argv[2];
	const theme = await import(pathToFileURL(join(dirname(source), "../../modes/interactive/theme/theme.ts")).href);
	theme.initTheme("dark");
	return theme.theme;
})();

function restoreNumber(step: Step): void {
	if (step.number === undefined) return;
	if (step.kind === "call") step.args = { ...step.args, limit: Number(step.number) };
	else step.result = { ...step.result!, details: { ...step.result?.details, matchLimitReached: Number(step.number) } };
}

function renderStep(definition: Definition, step: Step, theme: unknown, previous: Component | undefined): Component {
	const context = {
		args: step.args, toolCallId: "grep-sequence", invalidate: () => {},
		lastComponent: step.wrongComponent ? { render: () => [] } : previous,
		state: {}, cwd: "/repo", executionStarted: true, argsComplete: true,
		isPartial: false, expanded: step.expanded ?? false, showImages: step.showImages ?? false, isError: step.isError ?? true,
	};
	return step.kind === "call"
		? definition.renderCall(step.args, theme, context)
		: definition.renderResult(step.result, { expanded: context.expanded, isPartial: false }, theme, context);
}

void runOracle(async (module, request) => {
	const theme = await themePromise;
	const output = [];
	for (const value of requireCases(request)) {
		const definition = (module as Api).createGrepToolDefinition("/repo");
		let call: Component | undefined;
		let result: Component | undefined;
		const frames = [];
		for (const step of value as Step[]) {
			restoreNumber(step);
			const previous = step.kind === "call" ? call : result;
			try {
				const component = renderStep(definition, step, theme, previous);
				if (step.kind === "call") call = component;
				else result = component;
				frames.push({ lines: component.render(100), reused: component === previous });
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				frames.push({ error: { name: error.name, message: error.message } });
			}
		}
		output.push(frames);
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep render sequence failed: ${String(error)}\n`);
	process.exitCode = 1;
});
