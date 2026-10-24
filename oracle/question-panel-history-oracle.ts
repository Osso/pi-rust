import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Panel = {
	focused: boolean;
	handleInput(key: string): void;
	invalidate(): void;
	render(width: number): string[];
};
type Api = {
	createAskQuestionsPanel(options: {
		questions: unknown;
		theme: unknown;
		keybindings: unknown;
		requestRender(): void;
		done(value: unknown): void;
	}): Panel;
};
type Step = {
	key?: string;
	focused?: boolean;
	width?: number;
	invalidate?: boolean;
};

async function loadContext() {
	const source = process.argv[2];
	if (!source)
		throw new Error("question panel oracle requires a source module");
	const root = dirname(dirname(source));
	const theme = (await import(
		pathToFileURL(join(root, "../modes/interactive/theme/theme.ts")).href
	)) as {
		initTheme(name: string): void;
		theme: unknown;
	};
	const keys = (await import(
		pathToFileURL(join(root, "keybindings.ts")).href
	)) as {
		KeybindingsManager: new (bindings?: unknown) => unknown;
	};
	theme.initTheme("dark");
	return { theme: theme.theme, KeybindingsManager: keys.KeybindingsManager };
}

const context = loadContext();
void runOracle(async (module, request) => {
	const results: unknown[] = [];
	let renders = 0;
	const active = await context;
	const panel = (module as Api).createAskQuestionsPanel({
		questions: request.questions,
		theme: active.theme,
		keybindings: new active.KeybindingsManager(request.bindings),
		requestRender: () => {
			renders++;
		},
		done: (value) => {
			results.push(value);
		},
	});
	return requireCases(request).map((value) => {
		const step = value as Step;
		if (step.focused !== undefined) panel.focused = step.focused;
		if (step.key !== undefined) panel.handleInput(step.key);
		if (step.invalidate) panel.invalidate();
		return {
			lines: panel.render(step.width ?? 80),
			focused: panel.focused,
			renders,
			results: [...results],
		};
	});
}).catch((error: unknown) => {
	process.stderr.write(`question panel oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
