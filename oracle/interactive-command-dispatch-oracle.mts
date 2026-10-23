import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { text: string };
		const prompts: string[] = [];
		const defaultEditor: { onSubmit?: (text: string) => Promise<void> } = {};
		const receiver = {
			defaultEditor,
			editor: { setText: () => {} },
			session: { isCompacting: false, isStreaming: false },
			closeResponseCompleteNotification: () => {},
			addSubmittedTextToHistory: () => {},
			flushPendingBashComponents: () => {},
			onInputCallback: (text: string) => prompts.push(text),
		};
		InteractiveMode.prototype.setupEditorSubmitHandler.call(receiver);
		if (!defaultEditor.onSubmit) throw new Error("submit handler not installed");
		await defaultEditor.onSubmit(request.text);
		console.log(JSON.stringify({ ok: true, prompts }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
