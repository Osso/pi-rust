import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createBashToolDefinition(cwd: string): { renderResult(result: unknown, options: unknown, theme: unknown, context: unknown): unknown } };
const themePromise = (async () => {
	const theme = await import(pathToFileURL(join(dirname(process.argv[2]), "../../modes/interactive/theme/theme.ts")).href);
	theme.initTheme("dark");
	return theme.theme;
})();
const delay = () => new Promise(resolve => setTimeout(resolve, 1250));
void runOracle(async (module) => {
	const theme = await themePromise;
	const definition = (module as Api).createBashToolDefinition("/repo");
	const state = { startedAt: 0, endedAt: 1250 };
	let ticks = 0;
	const context = { state, invalidate: () => { ticks++; }, showImages: false, isError: false };
	const render = (partial: boolean) => definition.renderResult({ content: [] }, { expanded: true, isPartial: partial }, theme, context);
	const counts = [];
	render(true);
	render(true);
	await delay();
	counts.push(ticks);
	render(false);
	await delay();
	counts.push(ticks);
	render(true);
	await delay();
	counts.push(ticks);
	context.isError = true;
	render(true);
	await delay();
	counts.push(ticks);
	context.isError = false;
	render(true);
	// Let the interval expire without yielding the owner, then cancel it.
	Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1250);
	render(false);
	await delay();
	counts.push(ticks);
	return counts;
}).catch((error: unknown) => {
	process.stderr.write(`bash timer oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
