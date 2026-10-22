import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const utils = await import(pathToFileURL(path.join(source, "utils.ts")).href);
	const { Input } = await import(pathToFileURL(path.join(source, "components/input.ts")).href);
	const { Text } = await import(pathToFileURL(path.join(source, "components/text.ts")).href);
	const native = (units: number[]): string => String.fromCharCode(...units);
	
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			let output: unknown;
			if (request.operation === "utils") {
				const text = native(request.units);
				output = {
					width: utils.visibleWidth(text),
					wrap: utils.wrapTextWithAnsi(text, request.width),
					truncate: utils.truncateToWidth(text, request.width),
					slice: utils.sliceByColumn(text, request.start, request.width, true),
					segments: utils.extractSegments(text, request.start, request.start + 1, request.width, true),
					normalize: utils.normalizeTerminalOutput(text),
				};
			} else if (request.operation === "text") {
				const text = new Text(native(request.units), request.paddingX ?? 0, request.paddingY ?? 0);
				if (request.background) text.setCustomBgFn((value: string) => `\x1b[44m${value}\x1b[0m`);
				output = [text.render(request.width), text.render(request.width)];
			} else if (request.operation === "input") {
				const input = new Input({ mask: request.mask ?? false });
				const submitted: string[] = [];
				input.onSubmit = (value: string) => submitted.push(value);
				const states: unknown[] = [];
				for (const action of request.actions) {
					if (action.set) input.setValue(native(action.set));
					if (action.key) input.handleInput(native(action.key));
					states.push({ value: input.getValue(), cursor: (input as any).cursor, lines: input.render(request.width) });
				}
				output = { states, submitted };
			} else {
				throw new Error(`Unknown operation: ${request.operation}`);
			}
			// Preserve the source's native string serialization; no scalar conversion.
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();
