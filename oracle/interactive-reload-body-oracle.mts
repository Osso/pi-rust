import { createInterface } from "node:readline";
import { stripVTControlCharacters } from "node:util";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { ProjectTrustStore } = await import(pathToFileURL(`${root}/core/trust-manager.ts`).href);
const { SettingsManager } = await import(pathToFileURL(`${root}/core/settings-manager.ts`).href);
const { Container, Text } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const visible = (lines: string[]) => lines.map((line) => stripVTControlCharacters(line).trim()).filter(Boolean);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const colors = Object.fromEntries(request.colors.map((name: string) => [name, 0]));
		themes.setThemeInstance(new themes.Theme(colors, colors, "256color"));
		const messages: string[] = [];
		let preStart: unknown;
		const history: string[] = request.preStart ? ["keep history"] : [];
		const entries = request.preStart ? [{ type: "message", id: "user", parentId: null, timestamp: "2026-10-23T00:00:00.000Z", message: { role: "user", content: String.fromCharCode(...request.text), timestamp: 1000 } }] : request.entries ?? [];
		const settings = SettingsManager.inMemory();
		settings.setHideThinkingBlock(true);
		settings.setHttpIdleTimeoutMs(30000);
		settings.setEditorPaddingX(7);
		settings.setAutocompleteMaxVisible(9);
		settings.setShowHardwareCursor(true);
		settings.setClearOnShrink(true);
		if (request.trust) settings.setProjectTrusted(request.projectTrusted);
		let release!: () => void;
		let entered!: () => void;
		const held = new Promise<void>((resolve) => { release = resolve; });
		const started = new Promise<void>((resolve) => { entered = resolve; });
		const editor = {
			padding: 1, visible: 5,
			setPaddingX(value: number) { this.padding = value; },
			setAutocompleteMaxVisible(value: number) { this.visible = value; },
			render: () => ["editor"],
			addToHistoryWithoutPersistence(value: string) { history.unshift(value); },
		};
		const ui = { hardware: false, clear: false, setFocus: () => {}, requestRender: () => {},
			setShowHardwareCursor(value: boolean) { this.hardware = value; },
			setClearOnShrink(value: boolean) { this.clear = value; } };
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			options: {}, autoTrustOnReloadCwd: request.autoCwd,
			runtimeHost: { services: { agentDir: request.agentDir }, session: {
				sessionManager: { getCwd: () => request.cwd, buildContextEntries: () => entries },
				isStreaming: false, isCompacting: false,
				settingsManager: settings,
				resourceLoader: { getThemes: () => ({ themes: [] }) },
				extensionRunner: {}, modelRegistry: { getError: () => undefined },
			}, reload: async ({ beforeSessionStart }: { beforeSessionStart: () => void }) => {
				entered(); await held;
				if (request.error) throw new Error(String.fromCharCode(...request.error));
				beforeSessionStart();
				if (request.preStart) {
					preStart = { lines: visible(receiver.chatContainer.render(120)).map(units), hidden: receiver.hideThinkingBlock, history: history.map(units) };
					receiver.chatContainer.addChild(new Text("extension reload message", 0, 0));
				}
				if (request.header) {
					let expanded = false;
					let calls = 0;
					receiver.setExtensionHeader(() => {
						const component = new Text("header-collapsed", 0, 0);
						if (request.header === "plain") component.setText("header-plain");
						else component.setExpanded = (value: boolean) => {
							calls++;
							if (request.header === "throw" && calls === 2) throw new Error(String.fromCharCode(...request.headerError));
							expanded = value;
							component.setText(expanded ? "header-expanded" : "header-collapsed");
						};
						return component;
					});
					if (request.header !== "plain") receiver.customHeader.setText("header-collapsed");
				}
			} },
			editor, defaultEditor: editor, editorContainer: new Container(), chatContainer: new Container(), pendingTools: new Map(), ui,
			keybindings: { reload: () => {} }, themeController: { applyFromSettings: async () => {} },
			resetExtensionUI: () => { if (request.header) receiver.setExtensionHeader(undefined); }, rebuildChatFromMessages: () => {
				InteractiveMode.prototype.rebuildChatFromMessages.call(receiver);
			},
			setupAutocompleteProvider: () => {}, setupExtensionShortcuts: () => {}, showLoadedResources: () => {},
			maybeSaveImplicitProjectTrustAfterReload: () => request.trust ? InteractiveMode.prototype.maybeSaveImplicitProjectTrustAfterReload.call(receiver) : false,
			showStatus: (message: string) => messages.push(message), showError: (message: string) => messages.push(message),
		});
		if (request.header) {
			receiver.toolOutputExpanded = true;
			receiver.headerContainer = new Container();
			receiver.builtInHeader = new Text("", 0, 0);
			receiver.headerContainer.addChild(receiver.builtInHeader);
		}
		if (request.preStart) receiver.chatContainer.addChild(new Text("stale transcript", 0, 0));
		const reload = receiver.handleReloadCommand();
		await started;
		const loading = visible(receiver.editorContainer.render(80));
		release(); await reload;
		console.log(JSON.stringify({ ok: true, output: { loading, padding: editor.padding, visible: editor.visible,
			hardware: ui.hardware, clear: ui.clear, messages: [...visible(receiver.chatContainer.render(200)), ...messages].map(units),
			restored: receiver.editorContainer.children[0] === editor,
			...(request.trust ? { decision: new ProjectTrustStore(request.agentDir).get(request.cwd) } : {}),
			...(request.header ? { header: visible(receiver.headerContainer.render(80)) } : {}),
			...(request.preStart ? { preStart } : {}) } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
