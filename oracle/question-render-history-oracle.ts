import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Definition = {
	renderCall(args: unknown, theme: unknown, context: unknown): Component;
	renderResult(
		result: unknown,
		options: unknown,
		theme: unknown,
		context: unknown,
	): Component;
};
type Step = {
	kind: "call" | "result";
	args?: unknown;
	result?: unknown;
	last?: "text" | "foreign" | "none";
	reset?: boolean;
	width?: number;
};

async function loadContext() {
	const source = process.argv[2];
	if (!source)
		throw new Error("question renderer oracle requires a source module");
	const theme = (await import(
		pathToFileURL(
			join(dirname(source), "../../modes/interactive/theme/theme.ts"),
		).href
	)) as {
		initTheme(name: string): void;
		theme: unknown;
	};
	const tui = (await import(
		pathToFileURL(join(dirname(source), "../../../../tui/src/index.ts")).href
	)) as {
		Text: new (text: string, x: number, y: number) => Component;
	};
	theme.initTheme("dark");
	return { theme: theme.theme, Text: tui.Text };
}

const context = loadContext();
void runOracle(async (module, request) => {
	const api = module as { createAskQuestionsToolDefinition(): Definition };
	const definition = api.createAskQuestionsToolDefinition();
	const { theme, Text } = await context;
	let state: { callText?: Component; details?: unknown } = {};
	let call: Component | undefined;
	let result: Component | undefined;
	let args: unknown;
	return requireCases(request).map((value) => {
		const step = value as Step;
		if (step.reset) {
			state = {};
			call = undefined;
			result = undefined;
			args = undefined;
		}
		if (step.kind === "call") args = step.args;
		let previous = step.kind === "call" ? call : result;
		if (step.last === "foreign") previous = { render: () => ["other"] };
		if (step.last === "text") previous = new Text("retained result text", 0, 0);
		if (step.last === "none") previous = undefined;
		const width = step.width ?? 80;
		try {
			const renderContext = { state, args, lastComponent: previous };
			const component =
				step.kind === "call"
					? definition.renderCall(args, theme, renderContext)
					: definition.renderResult(
							step.result,
							{ expanded: true, isPartial: false },
							theme,
							renderContext,
						);
			if (step.kind === "call") call = component;
			else result = component;
			return {
				ok: true,
				reused: component === previous,
				lines: component.render(width),
				call: state.callText?.render(width) ?? null,
				details: state.details ?? null,
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			return { ok: false, name: error.name, message: error.message };
		}
	});
}).catch((error: unknown) => {
	process.stderr.write(`question renderer oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
