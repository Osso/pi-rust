// Differential oracle for interactive leaf helpers owned by ca-interactive-b.
// argv[2] is the pinned `packages/coding-agent/src` directory.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, any>;
type Request = { id: number; module: string; operation: string; [key: string]: any };

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

const handlers: Record<string, (api: Api, request: Request, sourceRoot: string) => unknown> = {
	elapsed: (api, request) => request.values.map((value: number) => api.formatElapsedDuration(value)),
	visualTruncate: (api, request) =>
		request.cases.map((value: any) => api.truncateToVisualLines(value.text, value.maxVisualLines, value.width, value.paddingX)),
	markdownTransform: (api, request) => {
		const transformers = request.transformers.map((kind: any) => {
			if (kind.kind === "upper") return (markdown: string) => markdown.toUpperCase();
			if (kind.kind === "append") return (markdown: string) => `${markdown}${kind.text}`;
			throw new Error(`unknown transformer ${kind.kind}`);
		});
		const transform = api.createMarkdownTransform(request.messageType, request.isStreaming, transformers);
		return request.inputs.map((input: string) => transform(input, request.availableWidth));
	},
	keyHints: (api, request) => ({
		formatKeyText: request.keys.map((value: any) => api.formatKeyText(value.key, value.options ?? {})),
		keyText: request.bindings.map((binding: string) => api.keyText(binding)),
		keyDisplayText: request.bindings.map((binding: string) => api.keyDisplayText(binding)),
		keyHint: request.hints.map((hint: any) => api.keyHint(hint.binding, hint.description)),
		rawKeyHint: request.rawHints.map((hint: any) => api.rawKeyHint(hint.key, hint.description)),
	}),
	modelSearch: (api, request) => request.items.map((item: any) => api.getModelSearchCandidates(item)),
	renderDiff: (api, request) => request.cases.map((value: string) => api.renderDiff(value)),
	configSelector: async (_api, request, sourceRoot) => {
		const settingsModule = (await import(pathToFileURL(`${sourceRoot}/core/settings-manager.ts`).href)) as Api;
		const selectorModule = (await import(
			pathToFileURL(`${sourceRoot}/modes/interactive/components/config-selector.ts`).href
		)) as Api;
		const storage = new settingsModule.InMemorySettingsStorage();
		storage.withLock("global", () => JSON.stringify({ packages: request.globalPackages }));
		const settingsManager = settingsModule.SettingsManager.fromStorage(storage, { projectTrusted: true });
		const metadata = {
			source: "npm:pi-tools",
			scope: "user",
			origin: "package",
			baseDir: request.packageRoot,
		};
		const resolved = {
			extensions: [
				{ path: `${request.packageRoot}/extensions/bar.ts`, enabled: true, metadata },
			],
			skills: [],
			prompts: [],
			themes: [],
		};
		const selector = new selectorModule.ConfigSelectorComponent(
			{ global: resolved, project: resolved },
			settingsManager,
			request.cwd,
			request.agentDir,
			() => {},
			() => {},
			() => {},
			24,
			request.writeScope,
			true,
		);
		const renders: string[][] = [];
		for (const input of request.inputs) {
			selector.getResourceList().handleInput(input);
			renders.push(selector.render(request.width));
		}
		return { projectPackages: settingsManager.getProjectSettings().packages ?? [], renders };
	},
};

const THEME_KEYS = [
	"accent", "border", "borderAccent", "borderMuted", "success", "error", "warning", "muted", "dim", "text",
	"thinkingText", "selectedBg", "userMessageBg", "userMessageText", "customMessageBg", "customMessageText",
	"customMessageLabel", "toolPendingBg", "toolSuccessBg", "toolErrorBg", "toolTitle", "toolOutput", "mdHeading",
	"mdLink", "mdLinkUrl", "mdCode", "mdCodeBlock", "mdCodeBlockBorder", "mdQuote", "mdQuoteBorder", "mdHr",
	"mdListBullet", "toolDiffAdded", "toolDiffRemoved", "toolDiffContext", "syntaxComment", "syntaxKeyword",
	"syntaxFunction", "syntaxVariable", "syntaxString", "syntaxNumber", "syntaxType", "syntaxOperator",
	"syntaxPunctuation", "thinkingOff", "thinkingMinimal", "thinkingLow", "thinkingMedium", "thinkingHigh",
	"thinkingXhigh", "bashMode",
];

function hex(value: number): string {
	return value.toString(16).padStart(2, "0");
}

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("interactive oracle requires the coding-agent src directory");
	const themeModule = (await import(pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href)) as Api;
	const colors = Object.fromEntries(
		THEME_KEYS.map((key, index) => [
			key,
			`#${hex((16 + 7 * index) % 256)}${hex((32 + 11 * index) % 256)}${hex((48 + 13 * index) % 256)}`,
		]),
	);
	themeModule.setThemeInstance(new themeModule.Theme(colors, colors, "truecolor"));
	const modules = new Map<string, Api>();
	const load = async (module: string): Promise<Api> => {
		let api = modules.get(module);
		if (!api) {
			api = (await import(pathToFileURL(`${sourceRoot}/${module}`).href)) as Api;
			modules.set(module, api);
		}
		return api;
	};
	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const handler = handlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			const output = await handler(await load(request.module), request, sourceRoot);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`interactive oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
