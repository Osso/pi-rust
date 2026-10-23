import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { ProjectTrustStore } = await import(pathToFileURL(`${root}/core/trust-manager.ts`).href);
const { SettingsManager } = await import(pathToFileURL(`${root}/core/settings-manager.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const visible = (lines: string[]) => lines.map((line) => line.replace(/\x1b\[[0-9;]*m/g, "").trim()).filter(Boolean);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.setThemeInstance(new themes.Theme(Object.fromEntries(request.colors.map((name: string) => [name, 0])), {}, "256color"));
		const messages: string[] = [];
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
		};
		const ui = { hardware: false, clear: false, setFocus: () => {}, requestRender: () => {},
			setShowHardwareCursor(value: boolean) { this.hardware = value; },
			setClearOnShrink(value: boolean) { this.clear = value; } };
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			options: {}, autoTrustOnReloadCwd: request.autoCwd,
			runtimeHost: { services: { agentDir: request.agentDir }, session: {
				sessionManager: { getCwd: () => request.cwd, buildContextEntries: () => request.entries ?? [] },
				isStreaming: false, isCompacting: false,
				settingsManager: settings,
				resourceLoader: { getThemes: () => ({ themes: [] }) },
				extensionRunner: {}, modelRegistry: { getError: () => undefined },
			}, reload: async ({ beforeSessionStart }: { beforeSessionStart: () => void }) => {
				entered(); await held;
				if (request.error) throw new Error(String.fromCharCode(...request.error));
				beforeSessionStart();
			} },
			editor, defaultEditor: editor, editorContainer: new Container(), chatContainer: new Container(), pendingTools: new Map(), ui,
			keybindings: { reload: () => {} }, themeController: { applyFromSettings: async () => {} },
			resetExtensionUI: () => {}, rebuildChatFromMessages: () => {
				InteractiveMode.prototype.rebuildChatFromMessages.call(receiver);
			},
			setupAutocompleteProvider: () => {}, setupExtensionShortcuts: () => {}, showLoadedResources: () => {},
			maybeSaveImplicitProjectTrustAfterReload: () => request.trust ? InteractiveMode.prototype.maybeSaveImplicitProjectTrustAfterReload.call(receiver) : false,
			showStatus: (message: string) => messages.push(message), showError: (message: string) => messages.push(message),
		});
		const reload = receiver.handleReloadCommand();
		await started;
		const loading = visible(receiver.editorContainer.render(80));
		release(); await reload;
		console.log(JSON.stringify({ ok: true, output: { loading, padding: editor.padding, visible: editor.visible,
			hardware: ui.hardware, clear: ui.clear, messages: [...visible(receiver.chatContainer.render(200)), ...messages].map((message) => Array.from({ length: message.length }, (_, i) => message.charCodeAt(i))),
			restored: receiver.editorContainer.children[0] === editor,
			...(request.trust ? { decision: new ProjectTrustStore(request.agentDir).get(request.cwd) } : {}) } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
