import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[]; invalidate(): void };
type Step = {
	kind: "call" | "result";
	args?: unknown;
	result?: unknown;
	expanded?: boolean;
	wrongComponent?: boolean;
	width?: number;
	invalidate?: boolean;
};
type Definition = {
	renderCall(args: unknown, theme: unknown, context: unknown): Component;
	renderResult(
		result: unknown,
		options: unknown,
		theme: unknown,
		context: unknown,
	): Component;
};
type Api = { createBashToolDefinition(cwd: string): Definition };
const themePromise = (async () => {
	const source = process.argv[2];
	const theme = await import(
		pathToFileURL(
			join(dirname(source), "../../modes/interactive/theme/theme.ts"),
		).href
	);
	const tui = await import("@earendil-works/pi-tui");
	theme.initTheme("dark");
	return { theme: theme.theme, Text: tui.Text, Container: tui.Container };
})();

void runOracle(async (module, request) => {
	const { theme, Text, Container } = await themePromise;
	return requireCases(request).map((value) => {
		const definition = (module as Api).createBashToolDefinition("/repo");
		const state = { startedAt: 0, endedAt: 1250 };
		let call: Component | undefined;
		let result: Component | undefined;
		return (value as Step[]).map((step) => {
			const isCall = step.kind === "call";
			const previous = isCall ? call : result;
			if (step.invalidate) previous?.invalidate();
			const context = {
				args: step.args,
				toolCallId: "bash-sequence",
				invalidate: () => {},
				state,
				lastComponent: step.wrongComponent
					? isCall
						? new Container()
						: new Text("wrong", 0, 0)
					: previous,
				cwd: "/repo",
				executionStarted: false,
				argsComplete: true,
				isPartial: false,
				expanded: step.expanded ?? false,
				showImages: false,
				isError: false,
			};
			try {
				const component = isCall
					? definition.renderCall(step.args, theme, context)
					: definition.renderResult(
							step.result,
							{ expanded: context.expanded, isPartial: false },
							theme,
							context,
						);
				const lines = component.render(step.width ?? 80);
				if (isCall) call = component;
				else result = component;
				return { lines, reused: component === previous };
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				return { error: { name: error.name, message: error.message } };
			}
		});
	});
}).catch((error: unknown) => {
	process.stderr.write(`bash render sequence failed: ${String(error)}\n`);
	process.exitCode = 1;
});
