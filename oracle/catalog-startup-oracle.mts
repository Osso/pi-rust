import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const argsApi = await import(pathToFileURL(`${root}/cli/args.ts`).href);
const catalog = await import(pathToFileURL(`${root}/core/model-catalog-cache.ts`).href);
const { ModelRegistry } = await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const { AuthStorage } = await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const { listModels } = await import(pathToFileURL(`${root}/cli/list-models.ts`).href);
serve(async (request) => {
	if (request.operation === "args") {
		const parsed = argsApi.parseArgs(request.args);
		return { ...parsed, unknownFlags: Array.from(parsed.unknownFlags.entries()) };
	}
	if (request.operation === "help") {
		let output = "";
		const original = process.stdout.write;
		process.stdout.write = ((chunk: string | Uint8Array) => {
			output += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
			return true;
		}) as typeof process.stdout.write;
		try { argsApi.printHelp(); } finally { process.stdout.write = original; }
		return output;
	}
	if (request.operation === "ensure_clock") {
		let clockCalls = 0;
		const now = () => {
			const timestamp = request.now + clockCalls * (request.tickMs ?? 0);
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
	const result = await catalog.loadOpenRouterCatalogAtStartup();
	const registry = ModelRegistry.create(AuthStorage.create(request.authPath), request.modelsPath);
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
	try { await listModels(registry, request.modelId); }
	finally { process.stdout.write = originalOut; process.stderr.write = originalErr; }
	return { model: registry.find("openrouter", request.modelId) ?? null, source: result.source, stdout, stderr };
});
