import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { installAssistantSnapshots } from "./assistant-snapshots.ts";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	await installAssistantSnapshots(dirname(dirname(process.argv[2])));
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const core = api.createFauxCore({
			api: "test-api",
			provider: "test-provider",
			models: [],
			tokenSize: request.tokenSize ?? { min: 1, max: 1 },
		});
		core.setResponses([
			request.reject === undefined
				? api.fauxAssistantMessage(request.content, { timestamp: 0 })
				: async () => { throw request.reject; },
		]);
		const stream = core.streamSimple(core.getModel(""), request.context, request.options ?? {});
		const events = [];
		for await (const event of stream)
			events.push(JSON.parse(JSON.stringify(event)));
		process.stdout.write(
			`${JSON.stringify({ events, result: await stream.result() })}\n`,
		);
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
