// Differential oracle for coding-agent interactive components.
// argv[2] is the pinned `packages/coding-agent/src` directory.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, any>;
type Request = { id: number; module: string; operation: string; [key: string]: any };

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

function errorWithStack(error: unknown): { name: string; message: string; stack?: string } {
	return error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : { name: "Unknown", message: String(error) };
}

const THEME_KEYS = [
	"accent", "border", "borderAccent", "borderMuted", "success", "error", "warning", "muted", "dim", "text", "thinkingText", "selectedBg", "userMessageBg", "userMessageText", "customMessageBg", "customMessageText", "customMessageLabel", "toolPendingBg", "toolSuccessBg", "toolErrorBg", "toolTitle", "toolOutput", "mdHeading", "mdLink", "mdLinkUrl", "mdCode", "mdCodeBlock", "mdCodeBlockBorder", "mdQuote", "mdQuoteBorder", "mdHr", "mdListBullet", "toolDiffAdded", "toolDiffRemoved", "toolDiffContext", "syntaxComment", "syntaxKeyword", "syntaxFunction", "syntaxVariable", "syntaxString", "syntaxNumber", "syntaxType", "syntaxOperator", "syntaxPunctuation", "thinkingOff", "thinkingMinimal", "thinkingLow", "thinkingMedium", "thinkingHigh", "thinkingXhigh", "bashMode",
];

function hex(value: number): string {
	return value.toString(16).padStart(2, "0");
}

function buildTheme(ThemeClass: any): unknown {
	const colors = Object.fromEntries(
		THEME_KEYS.map((key, index) => [
			key,
			`#${hex((16 + 7 * index) % 256)}${hex((32 + 11 * index) % 256)}${hex((48 + 13 * index) % 256)}`,
		]),
	);
	return new ThemeClass(colors, colors, "truecolor");
}

function applyInputs(component: { handleInput(data: string): void }, inputs: string[]): void {
	for (const input of inputs) component.handleInput(input);
}

const handlers: Record<string, (api: Api, request: Request) => unknown> = {
	elapsed: (api, request) => request.values.map((value: number) => api.formatElapsedDuration(value)),
	visualTruncate: (api, request) =>
		request.cases.map((value: any) => api.truncateToVisualLines(value.text, value.maxVisualLines, value.width, value.paddingX)),
	markdownTransform: (api, request) => {
		const transformers = request.transformers.map((kind: any) => {
			if (kind.kind === "upper") return (markdown: string) => markdown.toUpperCase();
			if (kind.kind === "append") return (markdown: string) => `${markdown}${kind.text}`;
			if (kind.kind === "drop") return () => undefined;
			if (kind.kind === "throw") return () => { throw new Error("boom"); };
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
	availableThemes: (api) => api.getAvailableThemes(),
	approval: (api, request) => {
		const events: unknown[] = [];
		const component = new api.ApprovalSelectorComponent({
			currentPreset: request.currentPreset,
			onSelect: (selection: unknown) => events.push(selection),
			onCancel: () => events.push({ cancel: true }),
		});
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
	sandbox: (api, request) => {
		const events: unknown[] = [];
		const component = new api.SandboxSelectorComponent({
			currentProfile: request.currentProfile,
			sessionProfile: request.sessionProfile ?? undefined,
			onSelect: (selection: unknown) => events.push(selection),
			onCancel: () => events.push({ cancel: true }),
		});
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
	trust: (api, request) => {
		const events: unknown[] = [];
		const component = new api.TrustSelectorComponent({
			cwd: request.cwd,
			savedDecision: request.savedDecision,
			projectTrusted: request.projectTrusted,
			onSelect: (selection: unknown) => events.push(selection),
			onCancel: () => events.push({ cancel: true }),
		});
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
	showImages: (api, request) => {
		const events: unknown[] = [];
		const component = new api.ShowImagesSelectorComponent(
			request.currentValue,
			(show: boolean) => events.push({ show }),
			() => events.push({ cancel: true }),
		);
		applyInputs(component.getSelectList(), request.inputs);
		return { lines: component.render(request.width), events };
	},
	thinking: (api, request) => {
		const events: unknown[] = [];
		const component = new api.ThinkingSelectorComponent(
			request.currentLevel,
			request.availableLevels,
			(level: string) => events.push({ level }),
			() => events.push({ cancel: true }),
		);
		applyInputs(component.getSelectList(), request.inputs);
		return { lines: component.render(request.width), events };
	},
	theme: (api, request) => {
		const events: unknown[] = [];
		const component = new api.ThemeSelectorComponent(
			request.currentTheme,
			(name: string) => events.push({ select: name }),
			() => events.push({ cancel: true }),
			(name: string) => events.push({ preview: name }),
		);
		applyInputs(component.getSelectList(), request.inputs);
		return { lines: component.render(request.width), events };
	},
	userMessage: (api, request) => {
		const events: unknown[] = [];
		const component = new api.UserMessageSelectorComponent(
			request.messages,
			(id: string) => events.push({ select: id }),
			() => events.push({ cancel: true }),
			request.initialSelectedId ?? undefined,
		);
		applyInputs(component.getMessageList(), request.inputs);
		return { lines: component.render(request.width), events };
	},
	oauthSelector: (api, request) => {
		const events: unknown[] = [];
		const providers = request.providers.map((value: any) => ({
			id: value.id,
			name: value.name,
			authType: value.authType,
			method: value.methodName ? { name: value.methodName } : undefined,
			status: value.status ?? undefined,
		}));
		const component = new api.OAuthSelectorComponent(
			request.mode,
			providers,
			(providerId: string, authType: string) => events.push({ select: providerId, authType }),
			() => events.push({ cancel: true }),
			request.initialSearchInput ?? undefined,
		);
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
	agentSwitcher: (api, request) => {
		const events: unknown[] = [];
		const agents = request.agents.map((value: any) => ({
			id: value.id,
			parentId: value.parentId ?? undefined,
			displayName: value.displayName,
			agentType: value.agentType ?? "worker",
			lifecycle: value.lifecycle ?? "running",
			slot: value.slot ?? undefined,
			createdAt: "2026-06-30T00:00:00.000Z",
			updatedAt: "2026-06-30T00:00:00.000Z",
			cwd: "/repo",
			permission: { narrowed: true, policy: "on-request" },
			revision: 1,
		}));
		const component = new api.AgentSwitcherComponent(
			agents,
			request.selectedAgentId ?? undefined,
			(agentId: string) => events.push({ select: agentId }),
			() => events.push({ cancel: true }),
			(agent: any) => events.push({ inactive: agent.id }),
		);
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
	extension: (api, request) => {
		const events: unknown[] = [];
		const component = new api.ExtensionSelectorComponent(
			request.title,
			request.options,
			(option: string) => events.push({ select: option }),
			() => events.push({ cancel: true }),
			{ onToggleToolsExpanded: () => events.push({ expand: true }) },
		);
		applyInputs(component, request.inputs);
		return { lines: component.render(request.width), events };
	},
};

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("interactive oracle requires the coding-agent src directory");
	const themeModule = (await import(pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href)) as Api;
	themeModule.setThemeInstance(buildTheme(themeModule.Theme));
	const keybindingsModule = (await import(pathToFileURL(`${sourceRoot}/core/keybindings.ts`).href)) as Api;
	const tuiModule = (await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href)) as Api;
	tuiModule.setKeybindings(new keybindingsModule.KeybindingsManager());
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
			const output = handler(await load(request.module), request);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorWithStack(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`interactive oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
