import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const clock = Date.now;
		Date.now = () => request.now ?? 1234;
		try {
			let output: unknown;
			if (request.operation === "messages") {
				output = api.convertMessages(request.model, request.context);
			} else if (request.operation === "tools") {
				output = api.convertTools(request.tools, request.legacy) ?? null;
			} else {
				let stop: unknown;
				try {
					stop = api.mapStopReason(request.reason);
				} catch (error) {
					stop = { error: error instanceof Error ? error.message : String(error) };
				}
				output = {
					max: api.clampGoogleMaxOutputTokens(request.maxTokens),
					managed: api.usesServerManagedThinking(request.modelId),
					requiresId: api.requiresToolCallId(request.modelId),
					thinking: api.isThinkingPart(request.part),
					signature: api.retainThoughtSignature(request.existing, request.incoming) ?? null,
					choice: api.mapToolChoice(request.choice),
					stop,
					rawStop: api.mapStopReasonString(request.reason),
				};
			}
			process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) })}\n`);
		} finally {
			Date.now = clock;
		}
	}
}
void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
