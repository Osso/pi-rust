import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ColorMode = "truecolor" | "256color";

type ThemeApi = {
	loadThemeFromPath(themePath: string, mode?: ColorMode): {
		name?: string;
		getColorMode(): ColorMode;
		getFgAnsi(color: string): string;
		getBgAnsi(color: string): string;
	};
	detectTerminalBackgroundFromEnv(options?: { env?: Record<string, string | undefined> }): unknown;
	resolveThemeSetting(themeSetting: string | undefined, terminalTheme: "dark" | "light"): string | undefined;
	getThemeForRgbColor(rgb: { r: number; g: number; b: number }): "dark" | "light";
	getLanguageFromPath(filePath: string): string | undefined;
};

const THEME_COLORS = [
	"accent",
	"border",
	"borderAccent",
	"borderMuted",
	"success",
	"error",
	"warning",
	"muted",
	"dim",
	"text",
	"thinkingText",
	"userMessageText",
	"customMessageText",
	"customMessageLabel",
	"toolTitle",
	"toolOutput",
	"mdHeading",
	"mdLink",
	"mdLinkUrl",
	"mdCode",
	"mdCodeBlock",
	"mdCodeBlockBorder",
	"mdQuote",
	"mdQuoteBorder",
	"mdHr",
	"mdListBullet",
	"toolDiffAdded",
	"toolDiffRemoved",
	"toolDiffContext",
	"syntaxComment",
	"syntaxKeyword",
	"syntaxFunction",
	"syntaxVariable",
	"syntaxString",
	"syntaxNumber",
	"syntaxType",
	"syntaxOperator",
	"syntaxPunctuation",
	"thinkingOff",
	"thinkingMinimal",
	"thinkingLow",
	"thinkingMedium",
	"thinkingHigh",
	"thinkingXhigh",
	"thinkingMax",
	"bashMode",
] as const;

const THEME_BACKGROUNDS = [
	"selectedBg",
	"userMessageBg",
	"customMessageBg",
	"toolPendingBg",
	"toolSuccessBg",
	"toolErrorBg",
] as const;

type LoadThemeCase = {
	name: string;
	content: string;
	mode?: ColorMode;
};

type HelpersCase = {
	colorfgbg?: string;
	rgb?: { r: number; g: number; b: number };
	themeSetting?: string | null;
	terminalTheme?: "dark" | "light";
	path?: string;
};

type Request =
	| { id: RequestId; operation: "load_theme"; cases: LoadThemeCase[] }
	| { id: RequestId; operation: "helpers"; cases: HelpersCase[] };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function loadTheme(api: ThemeApi, themeCase: LoadThemeCase, directory: string): unknown {
	const themePath = join(directory, `${themeCase.name}.json`);
	writeFileSync(themePath, themeCase.content);
	try {
		const theme = api.loadThemeFromPath(themePath, themeCase.mode);
		return {
			name: themeCase.name,
			ok: true,
			theme: {
				name: theme.name ?? null,
				colorMode: theme.getColorMode(),
				fg: THEME_COLORS.map((color) => [color, theme.getFgAnsi(color)]),
				bg: THEME_BACKGROUNDS.map((background) => [background, theme.getBgAnsi(background)]),
			},
		};
	} catch (error) {
		return { name: themeCase.name, ok: false, error: normalizeError(error) };
	}
}

function runHelpers(api: ThemeApi, helperCase: HelpersCase): unknown {
	const result: Record<string, unknown> = {};
	if (helperCase.colorfgbg !== undefined) {
		result.detection = api.detectTerminalBackgroundFromEnv({ env: { COLORFGBG: helperCase.colorfgbg } });
	}
	if (helperCase.rgb !== undefined) {
		result.theme = api.getThemeForRgbColor(helperCase.rgb);
	}
	if (helperCase.terminalTheme !== undefined) {
		result.resolved = api.resolveThemeSetting(helperCase.themeSetting ?? undefined, helperCase.terminalTheme) ?? null;
	}
	if (helperCase.path !== undefined) {
		result.language = api.getLanguageFromPath(helperCase.path) ?? null;
	}
	return result;
}

function execute(request: Request, api: ThemeApi, directory: string): unknown {
	if (request.operation === "load_theme") {
		return { results: request.cases.map((themeCase) => loadTheme(api, themeCase, directory)) };
	}
	return { results: request.cases.map((helperCase) => runHelpers(api, helperCase)) };
}

function executeRequestLine(line: string, api: ThemeApi, directory: string): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify({ id, ok: true, output: execute(request, api, directory) })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(api: ThemeApi, directory: string): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api, directory);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("theme oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ThemeApi;
	const directory = mkdtempSync(join(tmpdir(), "pi-theme-oracle-"));
	serveRequests(api, directory);
	process.on("exit", () => rmSync(directory, { recursive: true, force: true }));
}

void main().catch((error: unknown) => {
	process.stderr.write(`theme oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
