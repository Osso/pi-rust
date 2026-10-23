import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const visible = (lines: string[]) => lines.map((text) => text.replace(/\x1b\[[0-9;]*m/g, ""));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.initTheme("dark", false);
		const choices: unknown[] = [];
		const frames: string[][] = [];
		const errors: string[] = [];
		const editor = { render: () => ["editor"] };
		const editorContainer = new Container();
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			runtimeHost: { session: { model: {}, modelRegistry: { authStorage: {
				getOAuthProviders: () => [{ id: "q15-callback", name: "Fixture", usesCallbackServer: false }],
				login: async (_id: string, callbacks: { onSelect: (prompt: unknown) => Promise<string | undefined> }) => {
					const answer = callbacks.onSelect({ message: "Choose fixture", options: [{ id: "", label: "Empty ID" }, { id: "other", label: "Other" }] });
					frames.push(visible(editorContainer.render(80)));
					for (const key of request.keys) editorContainer.children[0].handleInput(key);
					const choice = await answer;
					choices.push({ defined: choice !== undefined, units: choice === undefined ? null : units(choice) });
					throw new Error("fixture stop");
				},
			} } } },
			editor, editorContainer,
			ui: { setFocus: () => {}, requestRender: () => {} },
			showError: (message: string) => errors.push(message),
		});
		await receiver.showLoginDialog("q15-callback", "Fixture");
		console.log(JSON.stringify({ ok: true, output: { choices, frames, errors: errors.map(units), restored: editorContainer.children[0] === editor } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
