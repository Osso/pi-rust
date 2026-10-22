import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";

async function captureStatus(source: string, units: number[]): Promise<unknown> {
	const module = await import(pathToFileURL(path.join(source, "modes/interactive/components/status-indicator.ts")).href);
	const raw = String.fromCharCode(...units);
	let requests = 0;
	const callbacks: string[] = [];
	const ui = {
		requestRender: () => requests++,
		requestComponentRender: () => requests++,
	};
	const colorMessage = (text: string): string => {
		callbacks.push(text);
		return `[${text}]`;
	};
	const status = new module.StatusIndicator("working", ui, (text: string) => `<${text}>`, colorMessage, raw);
	const snapshots: string[][] = [];
	for (const width of [1, 4, 12]) {
		snapshots.push(status.render(width));
		status.setMessage(`${raw}updated`);
		snapshots.push(status.render(width));
	}
	status.dispose();
	return [snapshots, callbacks, requests];
}

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const module = await import(pathToFileURL(path.join(source, "core/keybindings.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			let output: unknown;
			if (request.operation === "status") {
				output = await captureStatus(source, request.units);
			} else {
				const manager = module.KeybindingsManager.create(request.directory);
				output = [manager.getKeys("app.interrupt"), manager.getUserBindings()];
			}
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();
