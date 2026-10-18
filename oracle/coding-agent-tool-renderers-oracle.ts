import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type RenderCase = {
	tool: string;
	kind: "call" | "result";
	args?: Record<string, unknown>;
	result?: { content: unknown[]; details?: unknown };
	expanded?: boolean;
	isError?: boolean;
	showImages?: boolean;
	width?: number;
};

type Definition = {
	renderCall?: (args: unknown, theme: unknown, context: unknown) => { render(width: number): string[] };
	renderResult?: (
		result: unknown,
		options: unknown,
		theme: unknown,
		context: unknown,
	) => { render(width: number): string[] };
};

async function loadTheme(sourcePath: string): Promise<unknown> {
	const themeModule = (await import(
		pathToFileURL(join(dirname(sourcePath), "../../modes/interactive/theme/theme.ts")).href
	)) as { initTheme(name?: string): void; theme: unknown };
	themeModule.initTheme("dark");
	return themeModule.theme;
}

async function loadFactories(sourcePath: string): Promise<Record<string, (cwd: string) => Definition>> {
	const load = async (name: string) => (await import(pathToFileURL(join(dirname(sourcePath), name)).href)) as Record<string, unknown>;
	const [read, write, ls, grep, find, codeIndex, bash] = await Promise.all([
		load("read.ts"),
		load("write.ts"),
		load("ls.ts"),
		load("grep.ts"),
		load("find.ts"),
		load("code-index.ts"),
		load("bash.ts"),
	]);
	return {
		read: read.createReadToolDefinition as (cwd: string) => Definition,
		write: write.createWriteToolDefinition as (cwd: string) => Definition,
		ls: ls.createLsToolDefinition as (cwd: string) => Definition,
		grep: grep.createGrepToolDefinition as (cwd: string) => Definition,
		find: find.createFindToolDefinition as (cwd: string) => Definition,
		outline: codeIndex.createOutlineToolDefinition as (cwd: string) => Definition,
		symbol: codeIndex.createSymbolToolDefinition as (cwd: string) => Definition,
		references: codeIndex.createReferencesToolDefinition as (cwd: string) => Definition,
		bash: bash.createBashToolDefinition as (cwd: string) => Definition,
	};
}

const factories = loadFactories(process.argv[2] ?? "");
const theme = loadTheme(process.argv[2] ?? "");

void runOracle(async (_module, request) => {
	const available = await factories;
	const activeTheme = await theme;
	const cwd = String(request.cwd);
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as RenderCase;
		try {
			const factory = available[input.tool];
			if (!factory) throw new Error(`unknown tool ${input.tool}`);
			const definition = factory(cwd);
			const context = {
				args: input.args ?? {},
				toolCallId: "render-differential",
				invalidate: () => {},
				lastComponent: undefined,
				state: {},
				cwd,
				executionStarted: false,
				argsComplete: true,
				isPartial: false,
				expanded: input.expanded ?? true,
				showImages: input.showImages ?? true,
				isError: input.isError ?? false,
			};
			let component: { render(width: number): string[] };
			if (input.kind === "call") {
				if (!definition.renderCall) throw new Error(`${input.tool} has no renderCall`);
				component = definition.renderCall(input.args ?? {}, activeTheme, context);
			} else {
				if (!definition.renderResult) throw new Error(`${input.tool} has no renderResult`);
				component = definition.renderResult(
					input.result ?? { content: [] },
					{ expanded: context.expanded, isPartial: false },
					activeTheme,
					context,
				);
			}
			results.push({ ok: true, value: component.render(input.width ?? 80) });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent tool-renderers oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
