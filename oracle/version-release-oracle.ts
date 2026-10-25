// Actual frozen release API with scripted Fetch responses; no network access.
import { importSource, serve, type OracleRequest } from "./config-auth-oracle-support.ts";

type ReleaseApi = {
	getLatestPiRelease(version: string, options: { timeoutMs?: number }): Promise<unknown>;
	getLatestPiVersion(version: string, options: { timeoutMs?: number }): Promise<unknown>;
	checkForNewPiVersion(version: string): Promise<unknown>;
};
type RecordedRequest = { url: string; method: string; accept: string | null; userAgent: string | null; signal: boolean };

function snapshotError(error: unknown): unknown {
	if (!(error instanceof Error)) return { thrown: error };
	const code = (error as Error & { code?: unknown }).code;
	return { name: error.name, constructor: error.constructor.name, message: error.message, ...(code === undefined ? {} : { code }) };
}

function installFetch(request: OracleRequest, requests: RecordedRequest[]): void {
	globalThis.fetch = async (input, init) => {
		const parsed = new Request(input, init);
		requests.push({ url: parsed.url, method: parsed.method, accept: parsed.headers.get("accept"), userAgent: parsed.headers.get("user-agent"), signal: init?.signal != null });
		if (request.mode === "fetch-error") throw new TypeError("fetch failed");
		if (request.mode === "timeout") {
			// Keep this scripted transport alive while the source's unref'ed timer fires.
			return new Promise<Response>((_resolve, reject) => {
				const keepAlive = setTimeout(() => reject(new Error("timeout did not fire")), 1000);
				init?.signal?.addEventListener("abort", () => {
					clearTimeout(keepAlive);
					reject(init.signal?.reason);
				}, { once: true });
			});
		}
		if (request.mode === "slow") await new Promise((resolve) => setTimeout(resolve, 20));
		if (request.mode === "body-error") {
			return new Response(new ReadableStream({ start(controller) { controller.error(new Error("body failed")); } }));
		}
		return new Response(request.body as string ?? '{"version":"1.2.4"}', { status: request.status as number ?? 200 });
	};
}

function setFlag(name: string, value: unknown): void {
	if (typeof value === "string") process.env[name] = value;
	else delete process.env[name];
}

async function fetchRelease(api: ReleaseApi, request: OracleRequest): Promise<unknown> {
	setFlag("PI_SKIP_VERSION_CHECK", request.skip);
	setFlag("PI_OFFLINE", request.offline);
	const requests: RecordedRequest[] = [];
	installFetch(request, requests);
	const version = typeof request.currentJson === "string" ? JSON.parse(request.currentJson) as string : "1.2.3";
	const options = { timeoutMs: request.timeoutMs as number | undefined };
	try {
		let value: unknown;
		if (request.op === "version") value = await api.getLatestPiVersion(version, options);
		else if (request.op === "check") value = await api.checkForNewPiVersion(version);
		else value = await api.getLatestPiRelease(version, options);
		return { success: true, value: value ?? null, requests };
	} catch (error) {
		return { success: false, error: snapshotError(error), requests };
	}
}

async function main(): Promise<void> {
	const api = await importSource<ReleaseApi>();
	serve((request) => fetchRelease(api, request));
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
