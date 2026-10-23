// Frozen whole-provider reason boundaries using actual installed SDKs and scripted native fetch.
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";
import { installAssistantSnapshots } from "./assistant-snapshots.ts";
import { installScriptedFetch, setFetchScript, clearFetchScript } from "./scripted-fetch.ts";

type RecordValue = Record<string, any>;
function decode(input: RecordValue): unknown {
	if (input.kind === "default") return undefined;
	if (input.kind === "value") return input.value;
	const error = input.kind === "dom" ? new DOMException(input.message,input.name)
        : input.kind === "type-error" ? new TypeError(input.message)
        : Object.assign(new Error(input.message), { name:input.name ?? "Error" });
	Object.assign(error,input.properties);
	delete error.stack;
	return error;
}
async function main(): Promise<void> {
	const path = process.argv[2]!;
	await installAssistantSnapshots(join(dirname(path),".."));
	installScriptedFetch();
	const api = await import(pathToFileURL(path).href);
	serve(async (input: RecordValue) => {
		const controller = new AbortController();
		const supplied = input.caller === false ? undefined : controller.signal;
		if (input.mode === "pre") controller.abort(decode(input.failure));
		let observed: AbortSignal | undefined;
		let invoked = false;
		const delays: number[] = [];
		const nativeTimeout = AbortSignal.timeout;
		AbortSignal.timeout = (delay: number) => { delays.push(delay); return nativeTimeout(1); };
		setFetchScript(async (url: string | URL | Request, init?: RequestInit) => {
			invoked = true;
			observed = init?.signal ?? (url instanceof Request ? url.signal : undefined) ?? undefined;
			if (input.mode === "reject") throw decode(input.failure);
			if (input.mode === "body") return new Response(new ReadableStream({ start(c) { c.error(decode(input.failure)); } }), { headers: { "content-type":"text/event-stream" } });
			if (input.mode === "success") return new Response("", {headers:{"content-type":"text/event-stream"}});
			if (input.mode === "live") controller.abort(decode(input.failure));
			if (!observed) throw new Error("missing request signal");
			if (!observed.aborted) await new Promise<void>((resolve) => observed!.addEventListener("abort",()=>resolve(),{once:true}));
			throw observed.reason;
		});
		const keepAlive = setInterval(()=>{},1000);
		try {
			const stream = api.stream(input.model,input.context,{apiKey:"token",signal:supplied,maxRetries:0,timeoutMs:input.timeoutMs});
			for await (const _ of stream) { /* force the actual provider consumer */ }
			const message = await stream.result();
			if (input.abortAfter) controller.abort(decode(input.failure));
			return {
				stopReason:message.stopReason,errorMessage:message.errorMessage ?? null,invoked,delays,
				signal:observed ? {aborted:observed.aborted,reason:describeAbortReason(observed.reason)} : null,
			};
		} finally {
			clearInterval(keepAlive); AbortSignal.timeout = nativeTimeout; clearFetchScript();
		}
	});
}
void main().catch((error:unknown)=>{process.stderr.write(String(error));process.exitCode=1;});
