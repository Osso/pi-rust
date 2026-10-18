import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "shorten"; path: unknown }
	| { kind: "str"; value: unknown }
	| { kind: "tabs"; text: string }
	| { kind: "display"; text: string }
	| { kind: "textOutput"; content: Array<Record<string, unknown>>; showImages: boolean }
	| { kind: "invalidArg" }
	| { kind: "toolPath"; path: string | null; cwd: string; emptyFallback?: string };

type Api = {
	shortenPath(path: unknown): string;
	str(value: unknown): string | null;
	replaceTabs(text: string): string;
	normalizeDisplayText(text: string): string;
	getTextOutput(result: unknown, showImages: boolean): string;
	invalidArgText(theme: unknown): string;
	renderToolPath(rawPath: string | null, theme: unknown, cwd: string, options?: { emptyFallback?: string }): string;
};

const fakeTheme = {
	fg: (color: string, text: string) => `[fg:${color}]${text}[/]`,
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			const output =
				input.kind === "shorten"
					? api.shortenPath(input.path)
					: input.kind === "str"
						? api.str(input.value)
						: input.kind === "tabs"
							? api.replaceTabs(input.text)
							: input.kind === "display"
								? api.normalizeDisplayText(input.text)
								: input.kind === "textOutput"
									? api.getTextOutput({ content: input.content }, input.showImages)
									: input.kind === "invalidArg"
										? api.invalidArgText(fakeTheme)
										: api.renderToolPath(input.path, fakeTheme, input.cwd, { emptyFallback: input.emptyFallback });
			results.push({ ok: true, value: output });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? `${error.name}: ${error.message}` : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent render-utils oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
