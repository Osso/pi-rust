import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const core = api.createFauxCore({
			api: "test-api", provider: "test-provider", models: [], tokenSize: { min: 1, max: 1 },
		});
		core.setResponses([api.fauxAssistantMessage(request.content, { timestamp: 0 })]);
		const stream = core.streamSimple(core.getModel(""), request.context, {});
		const events = [];
		for await (const event of stream) events.push(JSON.parse(JSON.stringify(event)));
		process.stdout.write(`${JSON.stringify({ events, result: await stream.result() })}\n`);
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
