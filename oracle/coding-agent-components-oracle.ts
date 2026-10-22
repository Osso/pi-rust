import { dirname, join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Step = { key?: string; width?: number };

type Action = {
	kind: string;
	message?: string;
	placeholder?: string;
	lines?: string[];
	links?: Array<{ url: string; label?: string }>;
	showCloseHint?: boolean;
	info?: Record<string, unknown>;
};

type ComponentCase = {
	name: string;
	component: string;
	props: Record<string, unknown>;
	width: number;
	steps: Step[];
	actions?: Action[];
};

type Request = { id: RequestId; operation: "render"; cases: ComponentCase[] };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

type ComponentModule = {
	[key: string]: unknown;
};

async function importModule(directory: string, relativePath: string): Promise<ComponentModule> {
	return (await import(pathToFileURL(join(directory, relativePath)).href)) as ComponentModule;
}

function instantiate(
	modules: Record<string, ComponentModule>,
	component: string,
	props: Record<string, unknown>,
	events: string[],
): {
	component: { render(width: number): string[]; handleInput?(data: string): void };
	selectList?: { handleInput(data: string): void };
	applyAction?: (action: Action) => void;
} {
	const push = (event: string) => events.push(event);
	switch (component) {
		case "thinking": {
			const { ThinkingSelectorComponent } = modules.thinking;
			const instance = new (ThinkingSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				getSelectList(): { handleInput(data: string): void };
			})(
				props.currentLevel,
				props.availableLevels,
				(level: string) => push(`select:${level}`),
				() => push("cancel"),
			);
			return { component: instance, selectList: instance.getSelectList() };
		}
		case "theme": {
			const { ThemeSelectorComponent } = modules.theme;
			const instance = new (ThemeSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				getSelectList(): { handleInput(data: string): void };
			})(
				props.currentTheme,
				(name: string) => push(`select:${name}`),
				() => push("cancel"),
				(name: string) => push(`preview:${name}`),
			);
			return { component: instance, selectList: instance.getSelectList() };
		}
		case "show_images": {
			const { ShowImagesSelectorComponent } = modules.show_images;
			const instance = new (ShowImagesSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				getSelectList(): { handleInput(data: string): void };
			})(
				props.currentValue,
				(value: boolean) => push(`select:${value}`),
				() => push("cancel"),
			);
			return { component: instance, selectList: instance.getSelectList() };
		}
		case "extension": {
			const { ExtensionSelectorComponent } = modules.extension;
			const instance = new (ExtensionSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})(
				props.title,
				props.options,
				(option: string) => push(`select:${option}`),
				() => push("cancel"),
				{ onToggleToolsExpanded: () => push("toggle") },
			);
			return { component: instance };
		}
		case "approval": {
			const { ApprovalSelectorComponent } = modules.approval;
			const instance = new (ApprovalSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})({
				currentPreset: props.currentPreset,
				onSelect: (selection: { preset: string; scope: string }) =>
					push(`select:${selection.preset}:${selection.scope}`),
				onCancel: () => push("cancel"),
			});
			return { component: instance };
		}
		case "trust": {
			const { TrustSelectorComponent } = modules.trust;
			const instance = new (TrustSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})({
				cwd: props.cwd,
				savedDecision: props.savedDecision,
				projectTrusted: props.projectTrusted,
				onSelect: (selection: { trusted: boolean; updates: Array<{ path: string; decision: unknown }> }) =>
					push(`select:${selection.trusted}:${JSON.stringify(selection.updates)}`),
				onCancel: () => push("cancel"),
			});
			return { component: instance };
		}
		case "sandbox": {
			const { SandboxSelectorComponent } = modules.sandbox;
			const instance = new (SandboxSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})({
				currentProfile: props.currentProfile,
				sessionProfile: props.sessionProfile,
				onSelect: (selection: { profile?: string; scope: string }) =>
					push(`select:${selection.profile ?? "none"}:${selection.scope}`),
				onCancel: () => push("cancel"),
			});
			return { component: instance };
		}
		case "scoped_models": {
			const { ScopedModelsSelectorComponent } = modules.scoped_models;
			const models = (props.models as Array<Record<string, unknown>>).map((model) => ({
				id: model.id,
				name: model.name,
				provider: model.provider,
			}));
			const instance = new (ScopedModelsSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})(
				{
					allModels: models,
					enabledModelIds: props.enabledModelIds,
				},
				{
					onChange: (enabled: string[] | null) => push(`change:${JSON.stringify(enabled)}`),
					onPersist: (enabled: string[] | null) => push(`persist:${JSON.stringify(enabled)}`),
					onCancel: () => push("cancel"),
				},
			);
			return { component: instance };
		}
		case "oauth_selector": {
			const { OAuthSelectorComponent } = modules.oauth_selector;
			const providers = (props.providers as Array<Record<string, unknown>>).map((provider) => ({
				id: provider.id,
				name: provider.name,
				authType: provider.authType,
				method: provider.methodName ? { name: provider.methodName } : undefined,
				status: provider.status,
			}));
			const instance = new (OAuthSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})(
				props.mode,
				{ get: (id: string) => {
					const provider = (props.providers as Array<Record<string, unknown>>).find((provider) => provider.id === id);
					return provider?.storedType ? { type: provider.storedType } : undefined;
				} },
				providers,
				(id: string) => push(`select:${id}`),
				() => push("cancel"),
				(id: string) => providers.find((provider) => provider.id === id)?.status ?? {},
			);
			return { component: instance };
		}
		case "login_dialog": {
			const { LoginDialogComponent } = modules.login_dialog;
			const instance = new (LoginDialogComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
				showDeviceCode(info: unknown): void;
				showInfo(lines: string[]): void;
				showWaiting(message: string): void;
				showProgress(message: string): void;
				showPrompt(message: string, placeholder?: string): Promise<string>;
			})(
				{ requestRender: () => push("render"), invalidate: () => {} },
				props.providerId,
				(success: boolean, message?: string) => push(`complete:${success}:${message ?? ""}`),
				props.providerNameOverride,
				props.titleOverride,
			);
			return {
				component: instance,
				applyAction: (action: Action) => {
					switch (action.kind) {
						case "show_device_code":
							instance.showDeviceCode(action.info);
							return;
						case "show_info":
							instance.showInfo(action.lines ?? []);
							return;
						case "show_waiting":
							instance.showWaiting(action.message ?? "");
							return;
						case "show_progress":
							instance.showProgress(action.message ?? "");
							return;
						case "show_prompt":
							void instance.showPrompt(action.message ?? "", action.placeholder);
							return;
						default:
							throw new Error(`unknown action ${action.kind}`);
					}
				},
			};
		}
		case "agent_switcher": {
			const { AgentSwitcherComponent } = modules.agent_switcher;
			const instance = new (AgentSwitcherComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				handleInput(data: string): void;
			})(
				props.agents,
				props.selectedAgentId,
				(agentId: string) => push(`select:${agentId}`),
				() => push("cancel"),
				(agent: { id: string }) => push(`inactive:${agent.id}`),
			);
			return { component: instance };
		}
		case "user_message": {
			const { UserMessageSelectorComponent } = modules.user_message;
			const instance = new (UserMessageSelectorComponent as new (...args: unknown[]) => {
				render(width: number): string[];
				getMessageList(): { handleInput(data: string): void };
			})(
				props.messages,
				(id: string) => push(`select:${id}`),
				() => push("cancel"),
				props.initialSelectedId,
			);
			return { component: instance, selectList: instance.getMessageList() };
		}
		default:
			throw new Error(`unknown component ${component}`);
	}
}

function runCase(modules: Record<string, ComponentModule>, componentCase: ComponentCase): unknown {
	const events: string[] = [];
	const instance = instantiate(modules, componentCase.component, componentCase.props, events);
	for (const action of componentCase.actions ?? []) {
		instance.applyAction?.(action);
	}
	const input = (key: string) => {
		if (instance.selectList) {
			instance.selectList.handleInput(key);
		} else {
			instance.component.handleInput?.(key);
		}
	};
	let width = componentCase.width;
	const renders = [instance.component.render(width)];
	for (const step of componentCase.steps) {
		if (step.width !== undefined) {
			width = step.width;
		} else if (step.key !== undefined) {
			input(step.key);
		}
		renders.push(instance.component.render(width));
	}
	return { name: componentCase.name, renders, events };
}

function execute(request: Request, modules: Record<string, ComponentModule>): unknown {
	if (request.operation !== "render") {
		throw new Error(`unknown operation ${request.operation}`);
	}
	return { results: request.cases.map((componentCase) => runCase(modules, componentCase)) };
}

function executeRequestLine(line: string, modules: Record<string, ComponentModule>): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify({ id, ok: true, output: execute(request, modules) })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(modules: Record<string, ComponentModule>): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), modules);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceDirectory(): string {
	const sourceDirectory = process.argv[2];
	if (sourceDirectory === undefined) {
		throw new Error("components oracle requires a source directory");
	}
	return sourceDirectory;
}

async function main(): Promise<void> {
	const directory = readSourceDirectory();
	const tui = (await import("@earendil-works/pi-tui")) as {
		setCapabilities(capabilities: { images: null; trueColor: boolean; hyperlinks: boolean }): void;
	};
	tui.setCapabilities({ images: null, trueColor: true, hyperlinks: false });
	const keybindingsModule = (await import(
		pathToFileURL(join(dirname(dirname(dirname(directory))), "core", "keybindings.ts")).href
	)) as { KeybindingsManager: new () => unknown };
	const tuiModule = (await import("@earendil-works/pi-tui")) as {
		setKeybindings(keybindings: unknown): void;
	};
	tuiModule.setKeybindings(new keybindingsModule.KeybindingsManager());
	const themeModule = await importModule(dirname(directory), join("theme", "theme.ts"));
	(themeModule.initTheme as (name: string) => void)("dark");
	const modules: Record<string, ComponentModule> = {
		thinking: await importModule(directory, "thinking-selector.ts"),
		theme: await importModule(directory, "theme-selector.ts"),
		show_images: await importModule(directory, "show-images-selector.ts"),
		extension: await importModule(directory, "extension-selector.ts"),
		approval: await importModule(directory, "approval-selector.ts"),
		trust: await importModule(directory, "trust-selector.ts"),
		sandbox: await importModule(directory, "sandbox-selector.ts"),
		user_message: await importModule(directory, "user-message-selector.ts"),
		agent_switcher: await importModule(directory, "agent-switcher.ts"),
		login_dialog: await importModule(directory, "login-dialog.ts"),
		oauth_selector: await importModule(directory, "oauth-selector.ts"),
		scoped_models: await importModule(directory, "scoped-models-selector.ts"),
	};
	serveRequests(modules);
}

void main().catch((error: unknown) => {
	process.stderr.write(`components oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
