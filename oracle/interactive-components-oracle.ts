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
	keyHints: (api, request) => ({
		formatKeyText: request.keys.map((value: any) => api.formatKeyText(value.key, value.options ?? {})),
		keyText: request.bindings.map((binding: string) => api.keyText(binding)),
		keyDisplayText: request.bindings.map((binding: string) => api.keyDisplayText(binding)),
		keyHint: request.hints.map((hint: any) => api.keyHint(hint.binding, hint.description)),
		rawKeyHint: request.rawHints.map((hint: any) => api.rawKeyHint(hint.key, hint.description)),
	}),
	modelSearch: (api, request) => request.items.map((item: any) => api.getModelSearchCandidates(item)),
	renderDiff: (api, request) => request.cases.map((value: string) => api.renderDiff(value)),
	branchSummary: (api, request) => {
		const component = new api.BranchSummaryMessageComponent(
			{ role: "branchSummary", summary: request.summary, fromId: request.fromId, timestamp: request.timestamp },
			undefined,
		);
		component.setExpanded(request.expanded);
		return component.render(request.width);
	},
	compactionSummary: (api, request) => {
		const component = new api.CompactionSummaryMessageComponent(request.message, undefined);
		component.setExpanded(request.expanded);
		return component.render(request.width);
	},
	assistantMessage: (api, request) => {
		const transformers = request.transformers.map((kind: any) => {
			if (kind.kind === "upper") return (markdown: string) => markdown.toUpperCase();
			if (kind.kind === "append") return (markdown: string) => `${markdown}${kind.text}`;
			throw new Error(`unknown transformer ${kind.kind}`);
		});
		const component = new api.AssistantMessageComponent(
			request.message,
			request.hideThinkingBlock,
			undefined,
			request.hiddenThinkingLabel,
			request.outputPad,
			transformers,
		);
		if (request.isStreaming !== undefined) component.updateContent(request.message, request.isStreaming);
		return component.render(request.width);
	},
	userMessage: (api, request) => {
		const component = new api.UserMessageComponent(request.text, undefined, request.outputPad);
		if (request.inTransit !== undefined) component.setInTransit(request.inTransit);
		return component.render(request.width);
	},
	borderedLoader: async (_api, request, sourceRoot) => {
		const api = (await import(
			pathToFileURL(`${sourceRoot}/modes/interactive/components/bordered-loader.ts`).href
		)) as Api;
		const themeModule = (await import(
			pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href
		)) as Api;
		const loader = new api.BorderedLoader(
			{ requestComponentRender: () => false },
			themeModule.theme,
			request.message,
			{ cancellable: request.cancellable },
		);
		const lines = loader.render(request.width);
		loader.dispose();
		return lines;
	},
	statusIndicator: (api, request) => {
		const tui = { requestComponentRender: () => false };
		let indicator;
		if (request.kind === "working") {
			indicator = new api.WorkingStatusIndicator(tui, request.message);
		} else if (request.kind === "retry") {
			indicator = new api.RetryStatusIndicator(tui, request.attempt, request.maxAttempts, request.delayMs);
		} else if (request.kind === "compaction") {
			indicator = new api.CompactionStatusIndicator(tui, request.reason);
		} else if (request.kind === "branchSummary") {
			indicator = new api.BranchSummaryStatusIndicator(tui);
		} else if (request.kind === "idle") {
			indicator = new api.IdleStatus();
		} else {
			throw new Error(`unknown status kind ${request.kind}`);
		}
		const lines = indicator.render(request.width);
		indicator.dispose?.();
		return lines;
	},
	extensionInput: (api, request) => {
		const input = new api.ExtensionInputComponent(
			request.title,
			request.placeholder,
			() => {},
			() => {},
			{},
		);
		for (const data of request.inputs) input.handleInput(data);
		return input.render(request.width);
	},
	extensionEditor: async (api, request, sourceRoot) => {
		const fixedCell = { place: () => {}, clear: () => {}, update: () => true };
		const tui = {
			terminal: { rows: 24 },
			createFixedCell: () => fixedCell,
			requestRender: () => {},
			requestComponentRender: () => false,
			stop: () => {},
			start: () => {},
		};
		const keybindingsModule = (await import(
			pathToFileURL(`${sourceRoot}/core/keybindings.ts`).href
		)) as Api;
		const editor = new api.ExtensionEditorComponent(
			tui,
			new keybindingsModule.KeybindingsManager({}),
			request.title,
			request.prefill,
			() => {},
			() => {},
			{},
			"nano",
		);
		return editor.render(request.width);
	},
	firstTimeSetup: (api, request) => {
		const previews: string[] = [];
		const submitted: unknown[] = [];
		const component = new api.FirstTimeSetupComponent({
			detectedTheme: request.detectedTheme,
			onThemePreview: (theme: string) => previews.push(theme),
			onSubmit: (result: unknown) => submitted.push(result),
			onCancel: () => {},
		});
		const renders: string[][] = [component.render(request.width)];
		for (const data of request.inputs) {
			component.handleInput(data);
			renders.push(component.render(request.width));
		}
		return { renders, previews, submitted };
	},
	customEntryLifecycle: (api, request) => {
		const events: unknown[] = [];
		let generation = 0;
		const component = new api.CustomEntryComponent({ customType: request.customType }, (_entry: any, options: any) => {
			const current = ++generation;
			events.push(["build", current, options.expanded, options.sessionId]);
			if (!request.throws) events.push(["requested", options.requestRender()]);
			const cleanup = () => events.push(["cleanup", current]);
			options.registerCleanup(cleanup);
			options.registerCleanup(cleanup);
			if (request.throws) throw new Error(String.fromCharCode(...request.errorUnits));
			if (request.empty) return undefined;
			return { render: () => [`generation ${current}`], invalidate: () => events.push(["invalidate", current]), dispose: () => events.push(["dispose", current]) };
		}, { sessionId: request.sessionId, requestRender: () => { events.push(["request"]); return true; } });
		const frames = [component.render(request.width)];
		if (!request.throws) {
			component.setExpanded(true);
			frames.push(component.render(request.width));
			component.setExpanded(true);
			component.invalidate();
			frames.push(component.render(request.width));
		}
		component.dispose();
		component.dispose();
		return { frames: frames.map(lines => lines.map(line => Array.from({ length: line.length }, (_, i) => line.charCodeAt(i)))), events, hasContent: component.hasContent(), retired: component.render(request.width) };
	},
	customEntry: async (api, request, sourceRoot) => {
		const tuiModule = (await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href)) as Api;
		const entry = {
			type: "custom",
			customType: request.customType,
			data: null,
			id: "entry",
			parentId: null,
			timestamp: "2026-01-01T00:00:00.000Z",
		};
		const component = new api.CustomEntryComponent(entry, (value: any, options: any) => {
			return new tuiModule.Text(`custom ${value.customType} expanded=${options.expanded}`, 0, 0);
		}, { sessionId: "fixture", requestRender: () => false });
		component.setExpanded(request.expanded);
		return { lines: component.render(request.width), hasContent: component.hasContent() };
	},
	customEntryEmpty: (api, request) => {
		const entry = {
			type: "custom",
			customType: request.customType,
			data: null,
			id: "entry",
			parentId: null,
			timestamp: "2026-01-01T00:00:00.000Z",
		};
		const component = new api.CustomEntryComponent(entry, () => undefined, { sessionId: "fixture", requestRender: () => false });
		return { lines: component.render(request.width), hasContent: component.hasContent() };
	},
	customMessage: async (api, request, sourceRoot) => {
		const tuiModule = (await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href)) as Api;
		const message = {
			role: "custom",
			customType: request.customType,
			content: request.content,
			display: true,
			timestamp: 0,
		};
		const renderer = request.customRenderer
			? (_message: unknown, options: { expanded: boolean }) => {
				if (request.rendererThrows) throw new Error("fixture renderer throw");
				return new tuiModule.Text(`custom expanded=${options.expanded}`, 0, 0);
			}
			: undefined;
		const component = new api.CustomMessageComponent(message, renderer);
		component.setExpanded(request.expanded);
		return component.render(request.width);
	},
	bashExecution: (api, request) => {
		const component = new api.BashExecutionComponent(
			request.command,
			{ requestComponentRender: () => false },
			request.excludeFromContext,
			{ showElapsed: false },
		);
		for (const chunk of request.chunks) component.appendOutput(chunk);
		if (request.complete) {
			component.setComplete(request.exitCode, request.cancelled, request.truncation, request.fullOutputPath);
		}
		component.setExpanded(request.expanded);
		const lines = component.render(request.width);
		component.dispose();
		return { lines, output: component.getOutput(), command: component.getCommand() };
	},
	treeSelector: (api, request) => {
		const nodes = request.entries.map((entry: any) => ({ entry, children: [] }));
		const byId = new Map(nodes.map((node: any) => [node.entry.id, node]));
		const roots: any[] = [];
		for (const node of nodes) {
			const parent = node.entry.parentId == null ? undefined : byId.get(node.entry.parentId);
			if (parent) parent.children.push(node);
			else roots.push(node);
		}
		let copied: string | null = null;
		const selector = new api.TreeSelectorComponent(
			roots,
			request.currentLeafId,
			request.terminalHeight,
			() => {},
			() => {},
			undefined,
			request.initialSelectedId ?? undefined,
			request.initialFilterMode ?? undefined,
		);
		selector.onCopy = (text: string | undefined) => {
			copied = text ?? null;
		};
		const list = selector.getTreeList();
		for (const [entryId, label] of Object.entries(request.labels ?? {})) {
			list.updateNodeLabel(entryId, label);
		}
		const renders: string[][] = [];
		const selectedIds: Array<string | null> = [];
		for (const input of [null, ...request.inputs]) {
			if (input !== null) selector.handleInput(input);
			renders.push(selector.render(request.width));
			selectedIds.push(list.getSelectedNode()?.entry.id ?? null);
		}
		return { renders, selectedIds, copied };
	},
	sessionSelector: async (api, request, sourceRoot) => {
		const keybindingsModule = (await import(
			pathToFileURL(`${sourceRoot}/core/keybindings.ts`).href
		)) as Api;
		const toSession = (session: any) => ({
			...session,
			created: new Date(session.created),
			modified: new Date(session.modified),
		});
		const current = request.currentSessions.map(toSession);
		const all = request.allSessions.map(toSession);
		const archived = request.archivedSessions ? request.archivedSessions.map(toSession) : undefined;
		const selector = new api.SessionSelectorComponent(
			async () => current,
			async () => all,
			() => {},
			() => {},
			() => {},
			() => {},
			{
				keybindings: new keybindingsModule.KeybindingsManager({}),
				archivedSessionsLoader: archived ? async () => archived : undefined,
			},
			request.currentSessionFile ?? undefined,
		);
		const flush = () => new Promise<void>((resolve) => setImmediate(resolve));
		await flush();
		const renders: string[][] = [selector.render(request.width)];
		const selected: Array<string | null> = [selector.getSessionList().getSelectedSessionPath() ?? null];
		for (const input of request.inputs) {
			selector.handleInput(input);
			await flush();
			renders.push(selector.render(request.width));
			selected.push(selector.getSessionList().getSelectedSessionPath() ?? null);
		}
		return { renders, selected };
	},
	toolExecution: async (api, request, sourceRoot) => {
		const tui = (await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href)) as Api;
		const tools = (await import(pathToFileURL(`${sourceRoot}/core/tools/index.ts`).href)) as Api;
		const definition = request.definition
			? {
					name: request.definition.name,
					label: request.definition.name,
					description: "",
					parameters: { type: "object", properties: {} },
					execute: async () => ({ content: [] }),
					renderShell: request.definition.shell,
					renderCall: (_args: any, _theme: any, context: any) => {
						const text = context.lastComponent ?? new tui.Text("", 0, 0);
						text.setText(request.definition.call);
						return text;
					},
					renderResult: (_result: any, options: any, _theme: any, context: any) => {
						const text = context.lastComponent ?? new tui.Text("", 0, 0);
						text.setText(`${request.definition.result} expanded=${options.expanded}`);
						return text;
					},
				}
			: undefined;
		const ui = { requestRender() {} };
		const component = new api.ToolExecutionComponent(
			request.toolName,
			request.toolCallId,
			request.args,
			request.options ?? {},
			definition,
			ui,
			request.cwd,
		);
		const renders: string[][] = [];
		for (const event of request.events) {
			if (event.type === "args") component.updateArgs(event.args);
			if (event.type === "argsComplete") component.setArgsComplete();
			if (event.type === "expanded") component.setExpanded(event.expanded);
			if (event.type === "result") {
				component.updateResult(event.result, event.isPartial ?? false, event.finishedAt);
			}
			renders.push(component.render(request.width));
		}
		component.dispose();
		return renders;
	},
	toolRenderers: async (_api, request, sourceRoot) => {
		const themeModule = (await import(
			pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href
		)) as Api;
		const theme = themeModule.theme;
		const files: Record<string, string> = {
			change_working_directory: "change-working-directory",
			resume_session: "resume-session",
			search_current_session_history: "search-current-session-history",
			broadcast: "broadcast",
			channel_post: "channel-post",
			ask_questions: "ask-questions",
			ask_supervisor: "ask-supervisor",
			list_sessions: "list-sessions",
		};
		const modules = new Map<string, Api>();
		const definitionFor = async (tool: string): Promise<any> => {
			let api = modules.get(tool);
			if (!api) {
				api = (await import(
					pathToFileURL(`${sourceRoot}/core/tools/${files[tool]}.ts`).href
				)) as Api;
				modules.set(tool, api);
			}
			const pascal = tool
				.split("_")
				.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
				.join("");
			return api[`create${pascal}ToolDefinition`]();
		};
		const renders: string[][] = [];
		for (const item of request.cases) {
			const definition = await definitionFor(item.tool);
			const component =
				item.kind === "call"
					? definition.renderCall(item.args, theme, { lastComponent: undefined })
					: definition.renderResult(
							{ content: item.content, details: item.details, isError: item.isError ?? false },
							{ expanded: false, isPartial: false },
							theme,
							{ lastComponent: undefined },
						);
			renders.push(component.render(request.width));
		}
		return renders;
	},
	footerHelpers: (api, request) => ({
		tokens: request.tokens.map((count: number) => api.formatTokens(count)),
		cwd: request.cwdCases.map((value: any) => api.formatCwdForFooter(value.cwd, value.home)),
	}),
	footer: (api, request) => {
		const session = {
			state: {
				model: request.model,
				thinkingLevel: request.thinkingLevel,
			},
			sessionManager: {
				getSessionName: () => request.sessionName ?? undefined,
				getCwd: () => request.sessionCwd,
				getEntries: () => request.entries,
			},
			modelRuntime: { isUsingSubscription: () => request.usingSubscription ?? false },
			getContextUsage: () => request.contextUsage ?? undefined,
		};
		const statuses = new Map<string, string>(Object.entries(request.statuses ?? {}));
		const footerData = {
			getGitBranch: () => request.gitBranch ?? null,
			getExecutableName: () => request.executableName ?? undefined,
			getExtensionStatuses: () => statuses,
			getAvailableProviderCount: () => request.providerCount ?? 0,
		};
		const footer = new api.FooterComponent(session, footerData);
		footer.setAutoCompactEnabled(request.autoCompact);
		if (request.override) {
			const overrideManager = request.overrideSession
				? {
						getSessionName: () => request.overrideSession.sessionName,
						getCwd: () => request.overrideSession.sessionCwd,
						getEntries: () => request.overrideSession.entries,
					}
				: session.sessionManager;
			footer.setSessionOverride({
				cwd: request.override.cwd,
				sessionManager: overrideManager,
				model: request.override.model,
				thinkingLevel: request.override.thinkingLevel,
				contextUsage: request.override.contextUsage,
			});
		}
		return footer.render(request.width);
	},
	loginLifecycle: async (api, request, sourceRoot) => {
		const themeApi = await import(pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href);
		themeApi.initTheme("dark", false);
		const events: unknown[] = [];
		const answers: unknown[] = [];
		const dialog = new api.LoginDialogComponent({ requestRender: () => events.push(["render"]) }, "fixture", (success: boolean, message: string) => events.push(["complete", success, message]), undefined, "Fixture");
		const states: unknown[] = [];
		for (const step of request.steps) {
			if (step.op === "prompt" || step.op === "manual") {
				const index = answers.length;
				answers.push("pending");
				const promise = step.op === "prompt" ? dialog.showPrompt(step.message, step.placeholder) : dialog.showManualInput(step.message);
				promise.then((value: string) => { answers[index] = ["ok", value]; }, (error: Error) => { answers[index] = ["error", error.message]; });
			} else if (step.op === "input") dialog.handleInput(step.data);
			else if (step.op === "info") dialog.showInfo(step.lines);
			else if (step.op === "waiting") dialog.showWaiting(step.message);
			else if (step.op === "progress") dialog.showProgress(step.message);
			else if (step.op === "device") dialog.showDeviceCode(step.info);
			else if (step.op === "auth") dialog.showAuth(step.url, step.instructions);
			else if (step.op === "focus") dialog.focused = step.value;
			else throw new Error(`unknown login step ${step.op}`);
			await Promise.resolve();
			states.push({ frames: dialog.render(request.width).map((line: string) => Array.from({ length: line.length }, (_, i) => line.charCodeAt(i))), answers: structuredClone(answers), cancelled: dialog.signal.aborted, focused: dialog.focused });
		}
		return { states, events };
	},
	themeController: async (api, request, sourceRoot) => {
		const themeApi = await import(pathToFileURL(`${sourceRoot}/modes/interactive/theme/theme.ts`).href);
		const events: unknown[] = [];
		let setting = request.setting ?? undefined;
		let listener: (value: string) => void = () => {};
		const settings = { getThemeSetting: () => setting, setTheme: (value: string) => { setting = value; }, flush: async () => {} };
		const ui = {
			invalidate: () => events.push(["invalidate"]), requestRender: () => events.push(["render"]),
			setTerminalColorSchemeNotifications: (enabled: boolean) => events.push(["notifications", enabled]),
			onTerminalColorSchemeChange: (callback: (value: string) => void) => { listener = callback; events.push(["listen"]); },
			queryTerminalColorScheme: async () => { events.push(["scheme"]); return request.scheme; },
			queryTerminalBackgroundColor: async () => { events.push(["background"]); return request.background; },
		};
		const controller = new api.InteractiveThemeController(ui, settings, (error: string) => events.push(["error", error]), () => events.push(["changed"]));
		const states: unknown[] = [];
		for (const step of request.steps) {
			if (step.op === "apply") await controller.applyFromSettings();
			else if (step.op === "set") controller.setThemeName(step.name, step.showError ?? false);
			else if (step.op === "preview") controller.preview(step.name);
			else if (step.op === "instance") controller.setThemeInstance(themeApi.getThemeByName("light"));
			else if (step.op === "disable") controller.disableAutoSync();
			else if (step.op === "setting") setting = step.name;
			else if (step.op === "emit") listener(step.theme);
			else throw new Error(`unknown controller step ${step.op}`);
			states.push({ active: themeApi.theme.name ?? null, terminal: controller.getTerminalTheme(), setting: setting ?? null });
		}
		return { events, states };
	},
	autoThemeDetection: async (api, request) => {
		const events: unknown[] = [];
		const ui: Record<string, unknown> = {};
		const query = (name: string, value: unknown, fail: boolean) => async () => {
			events.push([name, "start"]);
			await new Promise(resolve => setTimeout(resolve, request.timeoutMs));
			events.push([name, "finish"]);
			if (fail) throw new Error(name);
			return value;
		};
		if (!request.missingScheme) ui.queryTerminalColorScheme = query("scheme", request.scheme, request.schemeError);
		ui.queryTerminalBackgroundColor = query("background", request.background, request.backgroundError);
		const theme = await api.detectTerminalThemeForAuto({ ui, timeoutMs: request.timeoutMs, env: request.env });
		return { theme, events };
	},
	renderRegionContainer: (api, request) => {
		const retired: number[] = [];
		const children = [0, 1, 2].map(id => ({ render: () => [`child ${id}`], invalidate: () => {}, dispose: () => retired.push(id) }));
		// An unstarted TUI cannot repaint a region. Screen-write success is tested with a real capture terminal.
		const container = new api.RenderRegionContainer({ createRenderRegion: () => ({ place: () => {}, clear: () => {}, tryRender: () => false, dispose: () => {} }) });
		const results: unknown[] = [];
		container.addChild(children[0]);
		results.push(container.requestChildRender(children[0]));
		container.trackScopedChild(children[0]);
		container.trackScopedChild(children[0]);
		container.trackScopedChild(children[1]);
		results.push(container.requestChildRender(children[1]));
		results.push(container.requestChildRender(children[0]));
		results.push(container.render(request.width));
		container.place({ row: 3, col: 1, width: request.width, height: 1 });
		results.push(container.requestChildRender(children[0]));
		container.removeChild(children[0]);
		container.removeChild(children[0]);
		results.push(container.requestChildRender(children[0]));
		container.addChild(children[2]);
		results.push(container.render(request.width));
		container.clear();
		container.clear();
		results.push(container.render(request.width));
		return { results, retired };
	},
	rootCompositor: (api, request) => {
		let height = request.sizes[0];
		let events: unknown[] = [];
		const rect = (value: any) => [value.row, value.col, value.width, value.height];
		const component = (label: string) => ({ render: () => [label], invalidate: () => {} });
		const onLayout = (label: string) => (layout: any) => events.push([label, rect(layout)]);
		const root = api.createInteractiveRootCompositor({
			getHeight: () => height, header: component("header"), loadedResources: component("resources"),
			chat: component("chat"), onChatLayout: onLayout("chat"), transcriptTail: component("tail"), pendingMessages: component("pending"),
			onTranscriptTailLayout: (layout: any) => events.push(["tail", rect(layout.rect), layout.flowEndRow, layout.bottomRow, layout.bottomHeight, layout.viewportHeight]),
			status: component("status"), onStatusLayout: onLayout("status"), widgetAbove: component("above"),
			onWidgetAboveLayout: request.widgetLayout ? onLayout("above") : undefined,
			editor: component("editor"), onEditorLayout: onLayout("editor"), widgetBelow: component("below"), footer: component("footer"),
		});
		return request.sizes.map((size: number) => {
			height = size;
			events = [];
			return { frame: root.render(request.width), events };
		});
	},
	editorRestore: async (api, request, sourceRoot) => {
		const tui = await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href);
		const children = request.texts.map((text: string) => new tui.Text(text, 0, 0));
		const container = new tui.Container();
		for (const child of children) container.addChild(child);
		let focused = children[request.focusIndex];
		const ui = {
			getFocusedComponent: () => focused,
			setFocus: (component: any) => { focused = component; },
			requestRender: () => {},
		};
		const restore = api.captureEditorDialogRestore(container, ui);
		const dialog = new tui.Text("dialog", 0, 0);
		container.clear();
		container.addChild(dialog);
		focused = dialog;
		children[0].setText(request.changedText);
		restore();
		const frames = [container.render(request.width)];
		const focus = container.children.indexOf(focused);
		container.clear();
		focused = undefined;
		restore();
		frames.push(container.render(request.width));
		return { frames, focus, focusAgain: container.children.indexOf(focused) };
	},
	coordinationComponent: async (api, request, sourceRoot) => {
		const tui = await import(pathToFileURL(`${sourceRoot}/../../tui/src/index.ts`).href);
		const custom = await import(pathToFileURL(`${sourceRoot}/modes/interactive/components/custom-message.ts`).href);
		const user = await import(pathToFileURL(`${sourceRoot}/modes/interactive/components/user-message.ts`).href);
		let hidden = false;
		const children = request.rich
			? [new user.UserMessageComponent(request.text), new custom.CustomMessageComponent(
				{ customType: "shared_channel", content: "custom", display: true, timestamp: 0 },
				(_message: any, options: any) => new tui.Text(`expanded:${options.expanded}`, 0, 0),
			), new tui.Text("ordinary", 0, 0)]
			: [new tui.Text(request.text, 0, 0)];
		const component = new api.CoordinationMessageComponent(children, () => hidden);
		return request.states.map((state: any) => {
			hidden = state.hidden;
			component.setExpanded(state.expanded ?? false);
			component.setInTransit(state.inTransit ?? false);
			return component.render(request.width);
		});
	},
	coordinationVisibility: (api, request) => ({
		previews: request.previews.map((preview: any) => api.isCoordinationPreview(preview)),
		messages: request.messages.map((message: any) => api.isCoordinationMessage(message)),
	}),
	configSelector: async (_api, request, sourceRoot) => {
		const settingsModule = (await import(pathToFileURL(`${sourceRoot}/core/settings-manager.ts`).href)) as Api;
		const selectorModule = (await import(
			pathToFileURL(`${sourceRoot}/modes/interactive/components/config-selector.ts`).href
		)) as Api;
		const storage = new settingsModule.InMemorySettingsStorage();
		storage.withLock("global", () => JSON.stringify(request.globalSettings));
		storage.withLock("project", () => JSON.stringify(request.projectSettings));
		const settingsManager = settingsModule.SettingsManager.fromStorage(storage, { projectTrusted: true });
		const resolved = request.resolved;
		const events: string[] = [];
		const selector = new selectorModule.ConfigSelectorComponent(
			resolved,
			settingsManager,
			request.cwd,
			request.agentDir,
			() => events.push("close"),
			() => events.push("exit"),
			() => events.push("render"),
			request.height,
		);
		const renders: string[][] = [selector.render(request.width)];
		for (const input of request.inputs) {
			selector.getResourceList().handleInput(input);
			renders.push(selector.render(request.width));
		}
		return {
			globalSettings: settingsManager.getGlobalSettings(),
			projectSettings: settingsManager.getProjectSettings(),
			events,
			renders,
		};
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
