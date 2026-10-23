import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const warnings: string[] = [];
		const receiver = { session: { isStreaming: request.streaming, isCompacting: request.compacting }, showWarning: (message: string) => warnings.push(message) };
		await InteractiveMode.prototype.handleReloadCommand.call(receiver);
		console.log(JSON.stringify({ ok: true, output: warnings }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
