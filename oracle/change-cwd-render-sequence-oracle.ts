import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Step = {
	args?: unknown;
	result?: unknown;
	incompatible?: boolean;
	reset?: boolean;
	width?: number;
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
type Api = { createChangeWorkingDirectoryToolDefinition(): Definition };

async function loadTheme(): Promise<unknown> {
	const source = process.argv[2];
	if (!source)
		throw new Error("change-cwd renderer oracle requires a source module");
	const api = (await import(
		pathToFileURL(
			join(dirname(source), "../../modes/interactive/theme/theme.ts"),
		).href
	)) as {
		initTheme(name: string): void;
		theme: unknown;
	};
	api.initTheme("dark");
	return api.theme;
}

const theme = loadTheme();
void runOracle(async (module, request) => {
	const definition = (
		module as unknown as Api
	).createChangeWorkingDirectoryToolDefinition();
	const activeTheme = await theme;
	if (request.operation === "definition") {
		return Object.fromEntries(
			Object.entries(definition).filter(
				([, value]) => typeof value !== "function",
			),
		);
	}
	let previous: Component | undefined;
	return requireCases(request).map((value) => {
		const step = value as Step;
		if (step.reset) previous = undefined;
		const lastComponent = step.incompatible
			? { render: () => ["other"] }
			: previous;
		try {
			// Frozen change-working-directory.ts:141-150 blindly calls setText on the retained component.
			const context = { lastComponent };
			const component =
				step.result === undefined
					? definition.renderCall(step.args, activeTheme, context)
					: definition.renderResult(
							step.result,
							{ expanded: true, isPartial: false },
							activeTheme,
							context,
						);
			const reused = component === previous;
			previous = component;
			return { ok: true, reused, lines: component.render(step.width ?? 100) };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			return { ok: false, name: error.name, message: error.message };
		}
	});
}).catch((error: unknown) => {
	process.stderr.write(`change-cwd renderer oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
