// Actual bundled modules with injected workers; no hosted network requests.
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type RecordValue = Record<string, any>;
function decode(input: RecordValue): unknown {
	if (input.kind === "default") return undefined;
	if (input.kind === "value") return input.value;
	return Object.assign(new Error(input.message), { name: input.name ?? "Error" });
}
void runOracle(async (web, input: RecordValue) => {
	const image = await import(pathToFileURL(join(dirname(dirname(dirname(process.argv[2]!))), "codex-image-generation/src/index.ts")).href);
	const controller = new AbortController();
	let observed: AbortSignal | undefined;
	const supplied = input.caller === false ? undefined : controller.signal;
	if (input.mode === "pre") controller.abort(decode(input.reason));
	const worker = async (request: RecordValue): Promise<unknown> => {
		observed = request.signal;
		if (input.mode === "live") controller.abort(decode(input.reason));
		if (input.mode === "success") {
			if (input.tool === "image") throw new Error("finished");
			return "finished";
		}
		if (input.tool === "web") return new Promise(() => {});
		if (!observed!.aborted) await new Promise<void>((resolve) => observed!.addEventListener("abort", () => resolve(), { once: true }));
		throw observed!.reason;
	};
	const timeoutMs = input.timeoutMs ?? 5;
	const tool = input.tool === "web"
		? web.createWebSearchToolDefinition({ runSearch: worker, timeoutMs })
		: image.createImageGenerationToolDefinition({ runGeneration: worker, timeoutMs });
	const ctx = {
		model: { id:"test", name:"test", api:"openai-codex-responses", provider:"openai-codex" },
		modelRegistry: { getApiKeyAndHeaders: async () => ({ ok:true, apiKey:"test-key" }) },
	};
	const keepAlive = setInterval(() => {},1000);
	let error: string | null = null;
	try {
		await tool.execute("call", input.tool === "web" ? { query:"query" } : { prompt:"prompt" }, supplied, undefined, ctx);
	} catch (failure) {
		error = failure instanceof Error ? failure.message : String(failure);
	} finally { clearInterval(keepAlive); }
	if (input.waitAfter) await new Promise<void>((resolve) => setTimeout(resolve,10));
	if (input.abortAfter) controller.abort(decode(input.reason));
	return { error, invoked:observed !== undefined, signal: observed ? { aborted:observed.aborted,reason:describeAbortReason(observed.reason) } : null };
}).catch((error: unknown) => { process.stderr.write(String(error)); process.exitCode=1; });
