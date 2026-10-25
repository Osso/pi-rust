import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const { model, options } = JSON.parse(line);
		try {
			api.streamSimple(model, { messages: [] }, options);
			process.stdout.write(`${JSON.stringify({ acquired: true })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ acquired: false, name: error instanceof Error ? error.name : "UnknownError", message: error instanceof Error ? error.message : String(error) })}\n`);
		}
	}
}
void main().catch((error) => { console.error(error); process.exitCode = 1; });
