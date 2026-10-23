// Real frozen catalog module; only HTTP is scripted. Files and public return values stay literal.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

type CatalogApi = {
	loadOpenRouterCatalogAtStartup(options: { now: () => Date }): Promise<unknown>;
	ensureModelCatalogFresh(options: { force?: boolean; now: () => Date; fetchImpl: typeof fetch }): Promise<unknown>;
	fetchOpenRouterCatalog(): Promise<unknown>;
	getOpenRouterCatalogModels(): unknown;
};

function snapshotError(error: unknown): unknown {
	if (!(error instanceof Error)) return error;
	return {
		name: error.name,
		message: error.message,
		...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "cause")),
		...("cause" in error ? { cause: snapshotError(error.cause) } : {}),
	};
}

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("catalog oracle requires source path");
	const api = await import(pathToFileURL(sourcePath).href) as CatalogApi;
	serve(async (request) => {
		const calls: unknown[] = [];
		const scriptedFetch: typeof fetch = async (input, init) => {
			calls.push({ url: String(input), signal: init?.signal instanceof AbortSignal });
			if (typeof request.failure === "string") {
				throw new TypeError(request.failure, { cause: request.failureCause });
			}
			return new Response(typeof request.body === "string" ? request.body : JSON.stringify(request.payload), {
				status: Number(Array.isArray(request.statuses)
					? request.statuses[Math.min(calls.length - 1, request.statuses.length - 1)]
					: request.status ?? 200),
				headers: (request.headers ?? undefined) as Record<string, string> | undefined,
			});
		};
		globalThis.fetch = scriptedFetch;
		const now = () => new Date(Number(request.now));
		const invoke = () => request.operation === "startup"
			? api.loadOpenRouterCatalogAtStartup({ now })
			: request.operation === "fetch"
				? api.fetchOpenRouterCatalog()
				: api.ensureModelCatalogFresh({ now, force: request.force as boolean | undefined, fetchImpl: scriptedFetch });
		try {
			const result = await invoke();
			const numericStates = request.inspectNumbers && Array.isArray(result)
				? result.map((model: { cost: { input: number } }) => ({
					type: typeof model.cost.input,
					finite: Number.isFinite(model.cost.input),
					negativeZero: Object.is(model.cost.input, -0),
				}))
				: undefined;
			const models = (result as { models?: Array<{ id: string; autoCompactionThreshold?: number }> }).models;
			const inspectedModel = models?.find((model) => model.id === request.inspectId);
			const modelWire = typeof request.inspectId === "string" ? JSON.stringify(inspectedModel) : undefined;
			const optionalNumberState = request.inspectOptional && inspectedModel
				? {
					present: "autoCompactionThreshold" in inspectedModel,
					type: typeof inspectedModel.autoCompactionThreshold,
					finite: Number.isFinite(inspectedModel.autoCompactionThreshold),
				}
				: undefined;
			return { result, calls, memoized: api.getOpenRouterCatalogModels(), numericStates, modelWire, optionalNumberState };
		} catch (error) {
			if (!request.inspectError) throw error;
			return { error: snapshotError(error), calls };
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`catalog oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
