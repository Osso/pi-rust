import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { installAssistantSnapshots } from "./assistant-snapshots.ts";
import {
	installScriptedFetch,
	setFetchScript,
	clearFetchScript,
} from "./scripted-fetch.ts";
import { pathToFileURL } from "node:url";

type Request = { model: unknown; sse: string; apiKey: string };
type Event = {
	type: string;
	delta?: string;
	content?: unknown;
	toolCall?: unknown;
};
type Provider = {
	stream(
		model: unknown,
		context: unknown,
		options: unknown,
	): AsyncIterable<Event> & { result(): Promise<unknown> };
};

function fakeResponse(sse: string): Response {
	const bytes = new TextEncoder().encode(sse);
	const body = new ReadableStream<Uint8Array>({
		start(controller) {
			for (const byte of bytes) controller.enqueue(Uint8Array.of(byte));
			controller.close();
		},
	});
	return new Response(body, {
		headers: { "content-type": "text/event-stream" },
	});
}

async function captureStream(
	provider: Provider,
	request: Request,
): Promise<string> {
	setFetchScript(async () => fakeResponse(request.sse));
	const stream = provider.stream(
		request.model,
		{ messages: [] },
		{
			apiKey: request.apiKey,
			maxRetries: 0,
			transport: "sse",
		},
	);
	const events = [];
	for await (const event of stream) {
		events.push({
			type: event.type,
			delta: event.delta,
			content: event.content,
			toolCall: event.toolCall,
		});
	}
	// Nested wire string prevents the Rust scalar control envelope from decoding content.
	return JSON.stringify({ events, result: await stream.result() });
}

async function main(): Promise<void> {
	installScriptedFetch();
	await installAssistantSnapshots(dirname(dirname(process.argv[2])));
	const provider = (await import(
		pathToFileURL(process.argv[2]).href
	)) as Provider;
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const wire = await captureStream(provider, JSON.parse(line) as Request);
			process.stdout.write(`${JSON.stringify({ wire })}\n`);
		} finally {
			clearFetchScript();
		}
	}
}

main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
