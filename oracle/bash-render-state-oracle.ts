import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Frame = {
	result: unknown;
	state?: Record<string, unknown>;
	expanded?: boolean;
	width?: number;
};
type Api = {
	createBashToolDefinition(cwd: string): {
		renderResult(result: unknown, options: unknown, theme: unknown, context: unknown): { render(width: number): string[] };
	};
};
const themePromise = (async () => {
	const source = process.argv[2];
	const module = await import(pathToFileURL(join(dirname(source), "../../modes/interactive/theme/theme.ts")).href);
	module.initTheme("dark");
	return module.theme;
})();

void runOracle(async (module, request) => {
	const theme = await themePromise;
	const definition = (module as Api).createBashToolDefinition("/repo");
	return requireCases(request).map((value) => {
		const frame = value as Frame;
		const state = { ...frame.state };
		const context = {
			args: {}, toolCallId: "bash-render-state", invalidate: () => {}, state,
			cwd: "/repo", executionStarted: false, argsComplete: true,
			isPartial: false, expanded: frame.expanded ?? true, showImages: true, isError: false,
		};
		const component = definition.renderResult(frame.result, { expanded: context.expanded, isPartial: false }, theme, context);
		return component.render(frame.width ?? 80);
	});
}).catch((error: unknown) => {
	process.stderr.write(`bash render state oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
