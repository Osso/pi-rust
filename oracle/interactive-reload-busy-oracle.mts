import { createInterface } from "node:readline";
import { stripVTControlCharacters } from "node:util";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.setThemeInstance(new themes.Theme({ warning: 0 }, {}, "256color"));
		const receiver = { session: { isStreaming: request.streaming, isCompacting: request.compacting }, chatContainer: new Container(), ui: { requestRender() {} }, showWarning: (message: string) => InteractiveMode.prototype.showWarning.call(receiver, message) };
		await InteractiveMode.prototype.handleReloadCommand.call(receiver);
		console.log(JSON.stringify({ ok: true, output: receiver.chatContainer.render(120).map((line: string) => stripVTControlCharacters(line).trim()).filter(Boolean) }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
