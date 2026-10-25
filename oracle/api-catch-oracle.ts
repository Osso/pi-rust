import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { installScriptedFetch, setFetchScript, clearFetchScript } from "./scripted-fetch.ts";

async function main(): Promise<void> {
	installScriptedFetch();
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const rejection = request.kind === "error"
			? Object.assign(new Error(request.value), request.properties)
			: request.value;
		let requests = 0;
		setFetchScript(async () => {
			requests++;
			throw new Error("unexpected fetch after payload rejection");
		});
		try {
			const options = { apiKey: "test-key", onPayload: () => { throw rejection; } };
			if (request.model.api === "openrouter-images") {
				const result = await api.generateImages(request.model, { input: [] }, options);
				process.stdout.write(`${JSON.stringify({ requests, events: [], result })}\n`);
				continue;
			}
			const stream = api.stream(request.model, { messages: [] }, options);
			const events = [];
			for await (const event of stream) events.push(event);
			process.stdout.write(`${JSON.stringify({ requests, events, result: await stream.result() })}\n`);
		} finally {
			clearFetchScript();
		}
	}
}
void main().catch((error) => { console.error(error); process.exitCode = 1; });
