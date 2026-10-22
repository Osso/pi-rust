import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]!);
	const { runAgentLoop } = await import(source.href);
	const { AssistantMessageEventStream } = await import(
		new URL("../../ai/src/utils/event-stream.ts", source).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const { scenario } = JSON.parse(line);
		const trace: string[] = [];
		const events: string[] = [];
		const controller = new AbortController();
		const message = {
			role: "assistant",
			api: "fixture",
			provider: "fixture",
			model: "fixture",
			content: [{ type: "text", text: "done" }],
			stopReason: "stop",
			timestamp: 0,
			usage: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
				totalTokens: 0,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
		};
		class Stream extends AssistantMessageEventStream {
			result() {
				return Promise.resolve(message);
			}
			[Symbol.asyncIterator]() {
				let index = 0;
				return {
					next: async () => {
						if (scenario === "next-error") throw new Error("next failed");
						if (scenario === "next-done") return { done: true };
						return index++ === 0
							? { done: false, value: { type: "start", partial: message } }
							: {
									done: false,
									value: { type: "done", reason: "stop", message },
								};
					},
					return: () => {
						trace.push("return");
						if (scenario === "sync-return-error")
							throw new Error("sync return failed");
						if (scenario === "close-abort") controller.abort();
						return Promise.resolve().then(() => {
							trace.push("closed");
							if (
								scenario === "return-error" ||
								scenario === "abort-return-error"
							)
								throw new Error("return failed");
							return { done: true };
						});
					},
				};
			}
		}
		let error: string | undefined;
		try {
			await runAgentLoop(
				[{ role: "user", content: "hello", timestamp: 0 }],
				{ messages: [], systemPrompt: "", tools: [] },
				{
					model: { id: "fixture", api: "fixture", provider: "fixture" },
					convertToLlm: (messages: any[]) => messages,
				},
				async (event: any) => {
					events.push(event.type);
					if (
						event.type === "message_start" &&
						event.message.role === "assistant"
					) {
						if (scenario === "abort" || scenario === "abort-return-error")
							controller.abort();
						if (scenario === "emit-error") throw new Error("sink failed");
					}
				},
				controller.signal,
				async () => new Stream(),
			);
		} catch (failure) {
			error = (failure as Error).message;
		}
		await new Promise((resolve) => setImmediate(resolve));
		console.log(
			JSON.stringify({
				trace,
				events,
				...(error === undefined ? {} : { error }),
			}),
		);
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
