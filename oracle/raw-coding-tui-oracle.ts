import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const module = await import(pathToFileURL(path.join(source, "core/keybindings.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			const manager = module.KeybindingsManager.create(request.directory);
			const output = [manager.getKeys("app.interrupt"), manager.getUserBindings()];
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();
