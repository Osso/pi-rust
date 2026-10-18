import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type ThemeEntry = [string, string];

type Case =
	| {
			kind: "highlight";
			name: string;
			code: string;
			language: string | null;
			ignoreIllegals: boolean;
			subset: string[] | null;
			theme: ThemeEntry[];
	  }
	| { kind: "render"; name: string; html: string; theme: ThemeEntry[] }
	| { kind: "supports"; name: string; language: string };

type Api = {
	highlight(
		code: string,
		options: { language?: string; ignoreIllegals?: boolean; languageSubset?: string[]; theme?: Record<string, unknown> },
	): string;
	renderHighlightedHtml(html: string, theme: Record<string, unknown>): string;
	supportsLanguage(name: string): boolean;
};

function buildTheme(entries: ThemeEntry[]): Record<string, (text: string) => string> {
	const theme: Record<string, (text: string) => string> = {};
	for (const [scope, label] of entries) {
		theme[scope] = (text) => `<${label}>${text}</${label}>`;
	}
	return theme;
}

void runOracle((module, request) => {
	const api = module as unknown as Api;
	return requireCases(request).map((value) => {
		const input = value as Case;
		if (input.kind === "render") {
			return { name: input.name, value: api.renderHighlightedHtml(input.html, buildTheme(input.theme)) };
		}
		if (input.kind === "supports") {
			return { name: input.name, value: api.supportsLanguage(input.language) };
		}
		const options: Record<string, unknown> = { ignoreIllegals: input.ignoreIllegals, theme: buildTheme(input.theme) };
		if (input.language !== null) options.language = input.language;
		if (input.subset !== null) options.languageSubset = input.subset;
		return { name: input.name, value: api.highlight(input.code, options) };
	});
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent syntax-highlight oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
