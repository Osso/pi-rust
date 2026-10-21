// Real frozen question panel, real frozen keybindings; no mutation of source checkout.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
async function main() {
	const root = process.argv[2];
	const { createAskQuestionsPanel } = await import(
		pathToFileURL(join(root, "core/tools/ask-questions-panel.ts")).href
	);
	const { KeybindingsManager } = await import(
		pathToFileURL(join(root, "core/keybindings.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		let result: unknown = null;
		const panel = createAskQuestionsPanel({
			questions: request.questions,
			theme: {
				fg: (_color: string, text: string) => text,
				bg: (_color: string, text: string) => text,
			},
			keybindings: new KeybindingsManager(),
			requestRender: () => {},
			done: (answer: unknown) => {
				result = answer;
			},
		});
		const renders = [panel.render(80).join("\n")];
		panel.focused = true;
		for (const key of request.keys) {
			panel.handleInput(key);
			renders.push(panel.render(80).join("\n"));
		}
		console.log(JSON.stringify({ result, renders }));
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
