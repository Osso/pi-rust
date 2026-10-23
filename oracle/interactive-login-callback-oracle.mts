import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Container, TUI, ProcessTerminal } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
type LoginCallbacks = {
	onSelect: (prompt: unknown) => Promise<string | undefined>;
	onAuth: (info: { url: string }) => void;
	onManualCodeInput: () => Promise<string>;
	onDeviceCode: (info: { userCode: string; verificationUri: string; intervalSeconds: number; expiresInSeconds: number }) => void;
	onProgress: (message: string) => void;
	onPrompt: (prompt: { message: string; placeholder?: string; allowEmpty: boolean }) => Promise<string>;
};
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.initTheme("dark", false);
		const choices: unknown[] = [];
		const frames: unknown[] = [];
		const errors: string[] = [];
		const editor = { render: () => ["editor"] };
		const editorContainer = new Container();
		const ui = new TUI(new ProcessTerminal());
		ui.requestRender = () => {};
		ui.addChild(editorContainer);
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			runtimeHost: { session: { model: {}, modelRegistry: { authStorage: {
				getOAuthProviders: () => [{ id: "q15-callback", name: "Fixture", usesCallbackServer: request.callbackServer ?? false }],
				login: async (_id: string, callbacks: LoginCallbacks) => {
					if (request.operation === "prompt") {
						callbacks.onDeviceCode({ userCode: "ABCD", verificationUri: "https://fixture.invalid/device", intervalSeconds: 0, expiresInSeconds: -0 });
						frames.push(editorContainer.render(80).map(units));
						callbacks.onProgress(String.fromCharCode(...request.progress));
						frames.push(editorContainer.render(80).map(units));
						const answer = callbacks.onPrompt({ message: String.fromCharCode(...request.message), placeholder: request.placeholder ? String.fromCharCode(...request.placeholder) : undefined, allowEmpty: false });
						frames.push(editorContainer.render(80).map(units));
						if (request.cancel) {
							editorContainer.children[0].handleInput("\x1b");
						} else {
							editorContainer.children[0].handleInput(String.fromCharCode(...request.code));
							editorContainer.children[0].handleInput("\r");
						}
						try { choices.push({ ok: units(await answer) }); }
						catch (error) { choices.push({ error: units((error as Error).message) }); }
					} else if (request.operation === "manual") {
						callbacks.onAuth({ url: "https://fixture.invalid/auth" });
						frames.push(editorContainer.render(80));
						if (request.callbackServer) {
							const answer = callbacks.onManualCodeInput();
							if (request.action === "cancel") {
								editorContainer.children[0].handleInput("\x1b");
								try { await answer; } catch (error) {
									choices.push({ error: units((error as Error).message) });
								}
							} else {
								editorContainer.children[0].handleInput(String.fromCharCode(...request.code));
								editorContainer.children[0].handleInput("\r");
								if (request.action === "empty") {
									let settled = false;
									void answer.then(() => { settled = true; }, () => { settled = true; });
									await Promise.resolve(); await Promise.resolve();
									choices.push({ pending: !settled });
								} else {
									choices.push(units(await answer));
									choices.push(units(await callbacks.onManualCodeInput()));
								}
							}
						}
					} else {
						const answer = callbacks.onSelect({ message: "Choose fixture", options: [{ id: "", label: "Empty ID" }, { id: "other", label: "Other" }] });
						frames.push(editorContainer.render(80));
						for (const key of request.keys) editorContainer.children[0].handleInput(key);
						const choice = await answer;
						choices.push({ defined: choice !== undefined, units: choice === undefined ? null : units(choice) });
					}
					throw new Error("fixture stop");
				},
			} } } },
			editor, editorContainer,
			ui,
			showError: (message: string) => errors.push(message),
		});
		await receiver.showLoginDialog("q15-callback", "Fixture");
		console.log(JSON.stringify({ ok: true, output: { choices, frames, errors: errors.map(units), restored: editorContainer.children[0] === editor } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
