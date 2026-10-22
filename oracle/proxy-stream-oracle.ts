import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
async function main(): Promise<void> {
	const { streamProxy } = await import(pathToFileURL(process.argv[2]!).href);
	Date.now = () => 0;
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const warnings: string[] = [];
		const requests: unknown[] = [];
		const events: unknown[] = [];
		const originalWarn = console.warn;
		console.warn = (...args: unknown[]) => {
			warnings.push(args.map(String).join(" "));
		};
		globalThis.fetch = async (_url, init) => {
			requests.push(JSON.parse(String(init?.body)));
			const wire =
				input.wire ??
				input.events
					.map((event: unknown) => `data: ${JSON.stringify(event)}\n\n`)
					.join("");
			const bytes = new TextEncoder().encode(wire);
			return new Response(
				new ReadableStream({
					start(controller) {
						for (
							let i = 0;
							i < bytes.length;
							i += input.chunkSize ?? bytes.length
						)
							controller.enqueue(
								bytes.slice(i, i + (input.chunkSize ?? bytes.length)),
							);
						controller.close();
					},
				}),
				{ status: input.status ?? 200, statusText: input.statusText ?? "OK" },
			);
		};
		const stream = streamProxy(input.model, input.context ?? { messages: [] }, {
			authToken: "token",
			proxyUrl: "https://proxy.invalid",
			...input.options,
		});
		// Capture each push before later shared partial mutations (approved immutable snapshot deviation).
		const push = stream.push.bind(stream);
		stream.push = (event: unknown) => {
			events.push(JSON.parse(JSON.stringify(event)));
			push(event);
		};
		for await (const _event of stream) {
		}
		const result = await stream.result();
		console.warn = originalWarn;
		console.log(JSON.stringify({ events, result, requests, warnings }));
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
