// Exercise the exact SDK versions resolved from the frozen provider module.
import { createRequire } from "node:module";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

const require = createRequire(process.argv[2]!);
const OpenAI = require("openai").default;
const Anthropic = require("@anthropic-ai/sdk").default;

function rejection(input: Record<string, unknown>): unknown {
	if (input.kind === "value") return input.value;
	if (input.kind === "dom") return new DOMException(input.message as string, input.name as string);
	const error = Object.assign(new Error(input.message as string), { name: input.name ?? "Error", ...input.properties as object });
	delete error.stack;
	return error;
}

serve(async (input) => {
	const controller = new AbortController();
	let observed: AbortSignal | undefined;
	const fetch = async (_url: unknown, init: RequestInit): Promise<Response> => {
		observed = init.signal ?? undefined;
		if (!observed) throw new Error("missing SDK signal");
		if (input.mode === "reject") throw rejection(input.failure as Record<string, unknown>);
		if (input.mode === "success") return new Response("data: [DONE]\n\n", { headers: { "content-type": "text/event-stream" } });
		if (input.mode === "caller") controller.abort("native\ud800");
		if (!observed.aborted) await new Promise<void>((resolve) => observed!.addEventListener("abort", () => resolve(), { once: true }));
		throw observed.reason;
	};
	const config = { apiKey: "token", baseURL: "http://sdk.invalid", fetch, maxRetries: 0, timeout: input.timeoutMs ?? 600000 };
	const signal = input.caller === false ? undefined : controller.signal;
	if (input.preAbort) controller.abort("pre\ud800");
	let error: unknown;
	try {
		if (input.sdk === "anthropic") {
			await new Anthropic(config).messages.create({ model: "test", max_tokens: 1, messages: [], stream: true }, { signal }).withResponse();
		} else {
			await new OpenAI(config).responses.create({ model: "test", input: [], stream: true }, { signal }).withResponse();
		}
	} catch (caught) {
		error = caught instanceof Error ? { name: caught.name, message: caught.message } : describeAbortReason(caught);
	}
	if (input.waitAfter) await new Promise<void>((resolve) => setTimeout(resolve, 10));
	if (input.abortAfter) controller.abort("native\ud800");
	return {
		error: error ?? null,
		sameAsCaller: observed !== undefined && observed === signal,
		fetchSignal: observed ? { aborted: observed.aborted, reason: describeAbortReason(observed.reason) } : null,
	};
});
