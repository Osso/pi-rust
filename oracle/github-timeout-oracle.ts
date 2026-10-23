// Actual frozen refresh export; record the 5s deadline and accelerate only its builtin clock.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]!).href) as { refreshGitHubCopilotToken(token: string): Promise<unknown> };
	const nativeTimeout = AbortSignal.timeout;
	serve(async () => {
		const delays: number[] = [];
		let signal: AbortSignal | undefined;
		AbortSignal.timeout = (delay: number) => { delays.push(delay); return nativeTimeout(1); };
		globalThis.fetch = (async (url: unknown, options: RequestInit) => {
			if (!String(url).endsWith("/models")) return new Response(JSON.stringify({ token: "copilot-token", expires_at: 1900000000 }));
			signal = options.signal ?? undefined;
			if (!signal) throw new Error("missing model deadline signal");
			if (!signal.aborted) await new Promise<void>((resolve) => signal!.addEventListener("abort", () => resolve(), { once: true }));
			throw signal.reason;
		}) as typeof fetch;
		const keepAlive = setInterval(() => {}, 1000);
		try {
			await api.refreshGitHubCopilotToken("old-refresh");
			throw new Error("refresh unexpectedly succeeded");
		} catch (error) {
			return { delays, rejection: describeAbortReason(error), signal: describeAbortReason(signal?.reason) };
		} finally {
			clearInterval(keepAlive);
			AbortSignal.timeout = nativeTimeout;
		}
	});
}
void main().catch((error: unknown) => { process.stderr.write(String(error)); process.exitCode = 1; });
