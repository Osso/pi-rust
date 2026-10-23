// Actual frozen refresh helper; accelerate only the builtin timeout's clock, record its requested delay.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

async function main(): Promise<void> {
const api = await import(pathToFileURL(process.argv[2]!).href) as {
	refreshAnthropicToken(token: string): Promise<unknown>;
};
const nativeTimeout = AbortSignal.timeout;
serve(async (input) => {
	const delays: number[] = [];
	let signal: AbortSignal | undefined;
	AbortSignal.timeout = (delay: number) => { delays.push(delay); return nativeTimeout(1); };
	globalThis.fetch = (async (_url: unknown, options: RequestInit) => {
		signal = options.signal ?? undefined;
		if (!signal) throw new Error("source did not supply timeout signal");
		if (input.failure) {
			const failure = input.failure as Record<string, unknown>;
			if (failure.kind === "value") throw failure.value;
			const error = Object.assign(new Error(failure.message as string), { name: failure.name ?? "Error", ...failure.properties as object });
			delete error.stack;
			throw error;
		}
		await new Promise<void>((resolve) => signal!.addEventListener("abort", () => resolve(), { once: true }));
		const rejection = signal.reason;
		// Approved omission: do not let V8 stack frames enter formatErrorDetails.
		if (rejection instanceof Error) delete rejection.stack;
		throw rejection;
	}) as typeof fetch;
	// Node's unref'ed AbortSignal timeout must not exit this deterministic observation early.
	const keepAlive = setInterval(() => {}, 1000);
	try {
		await api.refreshAnthropicToken("old-refresh");
		throw new Error("refresh unexpectedly succeeded");
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { delays, signal: describeAbortReason(signal?.reason), error: { name: error.name, message: error.message } };
	} finally {
		clearInterval(keepAlive);
		AbortSignal.timeout = nativeTimeout;
	}
});
}
void main().catch((error: unknown) => { process.stderr.write(String(error)); process.exitCode = 1; });
