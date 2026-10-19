import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type PreviewCase = {
	cwd: string;
	args: Record<string, unknown>;
	width?: number;
	result?: { content: unknown[]; details?: unknown };
	isError?: boolean;
};

type Component = { render(width: number): string[] };

type Definition = {
	renderCall?: (args: unknown, theme: unknown, context: unknown) => Component;
	renderResult?: (
		result: unknown,
		options: unknown,
		theme: unknown,
		context: unknown,
	) => Component;
};

async function loadTheme(sourcePath: string): Promise<unknown> {
	const themeModule = (await import(
		pathToFileURL(join(dirname(sourcePath), "../../modes/interactive/theme/theme.ts")).href
	)) as { initTheme(name?: string): void; theme: unknown };
	themeModule.initTheme("dark");
	return themeModule.theme;
}

async function loadEdit(sourcePath: string): Promise<(cwd: string) => Definition> {
	const edit = (await import(pathToFileURL(join(dirname(sourcePath), "edit.ts")).href)) as {
		createEditToolDefinition: (cwd: string) => Definition;
	};
	return edit.createEditToolDefinition;
}

async function waitForPreview(state: { callComponent?: { preview?: unknown } }): Promise<void> {
	const deadline = Date.now() + 5000;
	while (state.callComponent?.preview === undefined) {
		if (Date.now() > deadline) throw new Error("preview never completed");
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
}

const definitionFactory = loadEdit(process.argv[2] ?? "");
const theme = loadTheme(process.argv[2] ?? "");

void runOracle(async (_module, request) => {
	const createDefinition = await definitionFactory;
	const activeTheme = await theme;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as PreviewCase;
		const width = input.width ?? 80;
		const definition = createDefinition(input.cwd);
		const state: { callComponent?: { preview?: unknown } } = {};
		const context = {
			args: input.args,
			toolCallId: "edit-preview-differential",
			invalidate: () => {},
			lastComponent: undefined,
			state,
			cwd: input.cwd,
			executionStarted: false,
			argsComplete: true,
			isPartial: false,
			expanded: false,
			showImages: true,
			isError: input.isError ?? false,
		};
		if (!definition.renderCall) throw new Error("edit has no renderCall");
		const pending = definition.renderCall(input.args, activeTheme, context);
		const pendingLines = pending.render(width);
		await waitForPreview(state);
		const call = definition.renderCall(input.args, activeTheme, context);
		const callLines = call.render(width);
		let resultLines: string[] | undefined;
		if (input.result) {
			if (!definition.renderResult) throw new Error("edit has no renderResult");
			const component = definition.renderResult(
				input.result,
				{ expanded: false, isPartial: false },
				activeTheme,
				context,
			);
			resultLines = component.render(width);
		}
		results.push({ ok: true, pending: pendingLines, call: callLines, result: resultLines ?? null });
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent edit-preview oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
