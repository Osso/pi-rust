import { createInterface } from "node:readline";
import { stripVTControlCharacters } from "node:util";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { CustomEditor } = await import(pathToFileURL(`${root}/modes/interactive/components/custom-editor.ts`).href);
const { KeybindingsManager } = await import(pathToFileURL(`${root}/core/keybindings.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const identity = (text: string) => text;
const theme = { borderColor: identity, selectList: { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity } };
const host = { terminal: { rows: 24 }, requestRender() {}, createFixedCell() { return { place() {}, clear() {}, update() { return false; } }; } };
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const editor = new CustomEditor(host, theme, new KeybindingsManager());
		const contexts: unknown[] = [];
		themes.setThemeInstance(new themes.Theme({ error: 0 }, {}, "256color"));
		const diagnostics = { chatContainer: new Container(), ui: { requestRender() {} } };
		const handler = async (ctx: { cwd: string; hasUI: boolean; isIdle(): boolean; isProjectTrusted(): boolean; ui: { setEditorText(text: string): void } }) => {
			contexts.push({ cwd: ctx.cwd, hasUI: ctx.hasUI, idle: ctx.isIdle(), trusted: ctx.isProjectTrusted() });
			if (request.error) throw new Error(String.fromCharCode(...request.error));
			ctx.ui.setEditorText(String.fromCharCode(...request.text));
		};
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			defaultEditor: editor, keybindings: new KeybindingsManager(),
			runtimeHost: { session: { sessionManager: { getCwd: () => request.cwd }, isStreaming: false,
				settingsManager: { isProjectTrusted: () => true }, agent: {}, modelRegistry: {} } },
			createExtensionUIContext: () => ({ setEditorText: (text: string) => editor.setText(text) }),
			showError: (message: string) => InteractiveMode.prototype.showError.call(diagnostics, message),
		});
		receiver.setupExtensionShortcuts({ getShortcuts: () => new Map([["alt+q", { handler }]]), createContext: () => ({ sessionManager: {} }) });
		editor.handleInput("\x1bq");
		await new Promise((resolve) => setImmediate(resolve));
		console.log(JSON.stringify({ ok: true, output: { contexts, editor: units(editor.getText()), errors: diagnostics.chatContainer.render(200).map((line: string) => stripVTControlCharacters(line).trim()).filter(Boolean).map(units) } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
