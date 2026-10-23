// Real frozen catalog module; only HTTP is scripted. Files and public return values stay literal.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

type CatalogApi = {
	loadOpenRouterCatalogAtStartup(options: { now: () => Date }): Promise<unknown>;
	ensureModelCatalogFresh(options: { force?: boolean; now: () => Date; fetchImpl: typeof fetch }): Promise<unknown>;
	fetchOpenRouterCatalog(): Promise<unknown>;
	getOpenRouterCatalogModels(): unknown;
};

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("catalog oracle requires source path");
	const api = await import(pathToFileURL(sourcePath).href) as CatalogApi;
	serve(async (request) => {
		const calls: unknown[] = [];
		const scriptedFetch: typeof fetch = async (input, init) => {
			calls.push({ url: String(input), signal: init?.signal instanceof AbortSignal });
			return new Response(typeof request.body === "string" ? request.body : JSON.stringify(request.payload), {
				status: Number(request.status ?? 200),
				headers: (request.headers ?? undefined) as Record<string, string> | undefined,
			});
		};
		globalThis.fetch = scriptedFetch;
		const now = () => new Date(Number(request.now));
		const result = request.operation === "startup"
			? await api.loadOpenRouterCatalogAtStartup({ now })
			: request.operation === "fetch"
				? await api.fetchOpenRouterCatalog()
				: await api.ensureModelCatalogFresh({ now, force: request.force as boolean | undefined, fetchImpl: scriptedFetch });
		return { result, calls, memoized: api.getOpenRouterCatalogModels() };
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`catalog oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
