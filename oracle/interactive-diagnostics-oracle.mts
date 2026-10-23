import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const colors = Object.fromEntries(request.colors.map((name: string) => [name, 0]));
		themes.setThemeInstance(new themes.Theme(colors, colors, "256color"));
		const receiver = { chatContainer: new Container(), ui: { requestRender() {} } };
		const frames = [];
		for (const step of request.steps) {
			const message = String.fromCharCode(...step.message);
			if (step.kind === "error") InteractiveMode.prototype.showError.call(receiver, message);
			else InteractiveMode.prototype.showWarning.call(receiver, message);
			frames.push(receiver.chatContainer.render(request.width).map(units));
		}
		console.log(JSON.stringify({ ok: true, output: frames }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
