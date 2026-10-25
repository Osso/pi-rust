import { registerHooks } from "node:module";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
const packages = source.slice(0, source.indexOf("/packages/") + "/packages".length);
registerHooks({
	resolve(specifier, context, next) {
		if (specifier === "@earendil-works/pi-coding-agent") {
			return { url: pathToFileURL(`${packages}/coding-agent/src/modes/interactive/components/custom-editor.ts`).href, shortCircuit: true };
		}
		return next(specifier, context);
	},
});

let phase = "setup";
process.on("uncaughtException", (error: Error) => {
	process.stdout.write(`${JSON.stringify({ phase, name: error.name, message: error.message })}\n`);
	process.exit(17);
});

async function main(): Promise<void> {
	const { TUI } = await import(pathToFileURL(`${packages}/tui/src/index.ts`).href);
	const { KeybindingsManager } = await import(pathToFileURL(`${packages}/coding-agent/src/core/keybindings.ts`).href);
	const { initTheme, theme } = await import(pathToFileURL(`${packages}/coding-agent/src/modes/interactive/theme/theme.ts`).href);
	const { default: extension } = await import(pathToFileURL(source).href);
	initTheme("dark", false);
	const terminal = { rows: 24, columns: 80, write() {}, hideCursor() {}, showCursor() {} };
	const tui = new TUI(terminal as never, true);
	const identity = (text: string) => text;
	const editorTheme = { borderColor: identity, selectList: { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity } };
	const keybindings = new KeybindingsManager({});
	const handlers = new Map<string, Function>();
	let rejectTheme = false;
	const context = { cwd: "/tmp", model: undefined, getContextUsage: () => undefined, ui: {
		get theme() { if (rejectTheme) { phase = "render-theme-getter"; throw new Error("border-render-theme-failed"); } return theme; },
		setWorkingVisible() {}, setFooter() {},
		setEditorComponent(factory: Function) { const editor = factory(tui, editorTheme, keybindings); tui.addChild(editor); tui.setFocus(editor); },
	} };
	extension({ on(name: string, handler: Function) { handlers.set(name, handler); }, getThinkingLevel: () => "off", exec: async () => ({ stdout: "", stderr: "", code: 0 }) });
	await handlers.get("session_start")!({}, context);
	rejectTheme = true;
	phase = "scheduled-render";
	tui.requestRender();
	setTimeout(() => process.exit(23), 500);
}
void main().catch((error: unknown) => { console.error(error); process.exit(1); });
