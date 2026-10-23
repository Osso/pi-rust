import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const argsApi = await import(pathToFileURL(`${root}/cli/args.ts`).href);
const catalog = await import(pathToFileURL(`${root}/core/model-catalog-cache.ts`).href);
const { ModelRegistry } = await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const { AuthStorage } = await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const { listModels } = await import(pathToFileURL(`${root}/cli/list-models.ts`).href);
type Request = Record<string, unknown>;

async function captureOutput(action: () => Promise<void>) {
	let stdout = "";
	let stderr = "";
	const originalOut = process.stdout.write;
	const originalErr = process.stderr.write;
	process.stdout.write = ((chunk: string | Uint8Array) => {
		stdout += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
		return true;
	}) as typeof process.stdout.write;
	process.stderr.write = ((chunk: string | Uint8Array) => {
		stderr += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
		return true;
	}) as typeof process.stderr.write;
	try { await action(); } finally { process.stdout.write = originalOut; process.stderr.write = originalErr; }
	return { stdout, stderr };
}

async function runMain(request: Request, fetchImpl?: typeof fetch) {
	const mainApi = await import(pathToFileURL(`${root}/main.ts`).href);
	const originalExit = process.exit;
	const originalCode = process.exitCode;
	const originalFetch = globalThis.fetch;
	const originalCwd = process.cwd();
	const exitSignal = Symbol("fixture process.exit");
	let code: number | undefined;
	process.exit = ((value?: number | string | null) => { code = Number(value ?? process.exitCode ?? 0); throw exitSignal; }) as typeof process.exit;
	if (fetchImpl) globalThis.fetch = fetchImpl;
	process.chdir(request.cwd as string);
	try {
		const output = await captureOutput(() => mainApi.runCliActionOrReportError(async () => {
			try { await mainApi.main(request.args); } catch (error) { if (error !== exitSignal) throw error; }
		}));
		return { ...output, code: code ?? process.exitCode ?? 0 };
	} finally {
		process.exit = originalExit; process.exitCode = originalCode;
		globalThis.fetch = originalFetch; process.chdir(originalCwd);
	}
}

function snapshot(models: Array<{ id: string; provider: string }>, ids: string[]) {
	const found = ids.map((id) => models.find((model) => model.provider === "openrouter" && model.id === id) ?? null);
	return { models: found, wires: found.map((model) => model === null ? null : JSON.stringify(model)) };
}

async function graph(request: Request) {
	await catalog.loadOpenRouterCatalogAtStartup();
	const registry = ModelRegistry.create(AuthStorage.create(request.authPath), request.modelsPath);
	const ids = request.modelIds as string[];
	const before = snapshot(registry.getAll(), ids);
	const refresh = await runMain(request, async () => new Response(JSON.stringify(request.payload)));
	const memoAfterRefresh = snapshot(catalog.getOpenRouterCatalogModels(), ids);
	const stale = snapshot(registry.getAll(), ids);
	registry.refresh();
	const afterRefresh = snapshot(registry.getAll(), ids);
	const originalCacheHome = process.env.XDG_CACHE_HOME;
	try {
		process.env.XDG_CACHE_HOME = request.cacheHomeB as string;
		const rootResult = await catalog.loadOpenRouterCatalogAtStartup();
		const staleAfterRoot = snapshot(registry.getAll(), ids);
		registry.refresh();
		return { before, refresh, memoAfterRefresh, stale, afterRefresh, rootResult, staleAfterRoot, afterRoot: snapshot(registry.getAll(), ids) };
	} finally {
		if (originalCacheHome === undefined) delete process.env.XDG_CACHE_HOME;
		else process.env.XDG_CACHE_HOME = originalCacheHome;
	}
}

async function ensureClock(request: Request) {
	let clockCalls = 0;
	const now = () => {
		const timestamp = Number(request.now) + clockCalls * Number(request.tickMs ?? 0);
		clockCalls++;
		return new Date(request.invalidDate ? NaN : timestamp);
	};
	const fetchImpl: typeof fetch = async () => new Response(JSON.stringify(request.payload));
	try {
		const result = await catalog.ensureModelCatalogFresh({ force: request.force, now, fetchImpl });
		return { result, clockCalls };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { error: { name: error.name, message: error.message }, clockCalls };
	}
}

async function startup(request: Request) {
	const result = await catalog.loadOpenRouterCatalogAtStartup();
	const registry = ModelRegistry.create(AuthStorage.create(request.authPath), request.modelsPath);
	const output = await captureOutput(() => listModels(registry, request.modelId));
	return { model: registry.find("openrouter", request.modelId) ?? null, source: result.source, ...output };
}

serve(async (request) => {
	switch (request.operation) {
		case "args": {
			const parsed = argsApi.parseArgs(request.args);
			return { ...parsed, unknownFlags: Array.from(parsed.unknownFlags.entries()) };
		}
		case "help": return (await captureOutput(async () => { argsApi.printHelp(); })).stdout;
		case "ensure_clock": return ensureClock(request);
		case "main_offline_refresh": return runMain(request);
		case "graph": return graph(request);
		case "startup": return startup(request);
		default: throw new Error(`Unknown catalog fixture operation: ${request.operation}`);
	}
});
