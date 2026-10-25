// Actual frozen resolver and registry. Only getAvailable's list is overridden for algorithm fixtures.
import { pathToFileURL } from "node:url";
import { serve, type OracleRequest } from "./config-auth-oracle-support.ts";

type Model = { id: string; provider: string; name: string; baseUrl: string; reasoning: boolean; [key: string]: unknown };
type Registry = { getAll(): Model[]; getAvailable(): Model[] };
type Parsed = { model?: Model; thinkingLevel?: string; warning?: string; error?: string; fallbackMessage?: string };
type Scope = { scopedModels: Array<{ model: Model; thinkingLevel?: string }>; diagnostics: Array<{ type: "warning"; message: string; pattern: string }> };
type ResolverApi = {
	defaultModelPerProvider: Record<string, string>;
	findExactModelReferenceMatch(reference: string, models: Model[]): Model | undefined;
	parseModelPattern(pattern: string, models: Model[], options?: { allowInvalidThinkingLevelFallback?: boolean }): Parsed;
	resolveModelScopeWithDiagnostics(patterns: string[], registry: Registry): Promise<Scope>;
	resolveCliModel(options: Record<string, unknown>): Parsed;
	findInitialModel(options: Record<string, unknown>): Promise<Parsed>;
	restoreModelFromSession(provider: string, id: string, current: Model | undefined, print: boolean, registry: Registry): Promise<Parsed>;
};
type RegistryApi = { ModelRegistry: { inMemory(storage: unknown): Registry } };
type StorageApi = { AuthStorage: { inMemory(data: Record<string, unknown>): unknown } };

function normalizeModel(model: Model | undefined): Record<string, unknown> | null {
	return model ? { id: model.id, provider: model.provider, name: model.name, baseUrl: model.baseUrl, reasoning: model.reasoning } : null;
}
function normalizeParsed(result: Parsed): Record<string, unknown> {
	return { model: normalizeModel(result.model), thinkingLevel: result.thinkingLevel ?? null, warning: result.warning ?? null };
}
function fixtureRegistry(registryApi: RegistryApi, storageApi: StorageApi, request: OracleRequest, available?: Model[]): Registry {
	const data = Object.fromEntries(((request.authenticated ?? []) as string[]).map((provider) => [provider, { type: "api_key", key: "fixture" }]));
	const registry = registryApi.ModelRegistry.inMemory(storageApi.AuthStorage.inMemory(data));
	// Frozen getAll exposes the real backing array. This is test setup, not a production Rust identity requirement.
	const models = registry.getAll();
	models.splice(0, models.length, ...request.models as Model[]);
	if (available) Object.defineProperty(registry, "getAvailable", { value: () => available });
	return registry;
}
function normalizeScope(result: Scope): Record<string, unknown> {
	return {
		scopedModels: result.scopedModels.map((scoped) => ({ model: normalizeModel(scoped.model), thinkingLevel: scoped.thinkingLevel ?? null })),
		diagnostics: result.diagnostics.map(({ type, message, pattern }) => ({ type, message, pattern })),
	};
}
async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]);
	const api = await import(source.href) as ResolverApi;
	const registryApi = await import(new URL("model-registry.ts", source).href) as RegistryApi;
	const storageApi = await import(new URL("auth-storage.ts", source).href) as StorageApi;
	serve(async (request) => {
		const models = request.models as Model[];
		switch (request.operation) {
			case "defaults": return { defaults: api.defaultModelPerProvider };
			case "exact": return { model: normalizeModel(api.findExactModelReferenceMatch(request.reference as string, models)) };
			case "parse": return normalizeParsed(api.parseModelPattern(request.pattern as string, models, {
				allowInvalidThinkingLevelFallback: request.allowInvalidThinkingLevelFallback as boolean | undefined,
			}));
			case "scope": return normalizeScope(await api.resolveModelScopeWithDiagnostics(request.patterns as string[], fixtureRegistry(registryApi, storageApi, request, models)));
			case "cli": {
				const result = api.resolveCliModel({ ...request, modelRegistry: fixtureRegistry(registryApi, storageApi, request) });
				return { ...normalizeParsed(result), error: result.error ?? null };
			}
			case "restore": {
				const stdout: string[] = [];
				const stderr: string[] = [];
				const savedLog = console.log;
				const savedError = console.error;
				console.log = (...values: unknown[]) => { stdout.push(`${values.map(String).join(" ")}\n`); };
				console.error = (...values: unknown[]) => { stderr.push(`${values.map(String).join(" ")}\n`); };
				try {
					const result = await api.restoreModelFromSession(
						request.savedProvider as string, request.savedModelId as string,
						request.currentModel as Model | undefined, request.print as boolean,
						fixtureRegistry(registryApi, storageApi, request, request.available as Model[]),
					);
					return { model: result.model ?? null, fallbackMessage: result.fallbackMessage ?? null, stdout: stdout.join(""), stderr: stderr.join("") };
				} finally {
					console.log = savedLog;
					console.error = savedError;
				}
			}
			case "initial": {
				const result = await api.findInitialModel({ ...request, modelRegistry: fixtureRegistry(registryApi, storageApi, request, request.available as Model[]) });
				return { model: normalizeModel(result.model), thinkingLevel: result.thinkingLevel, fallbackMessage: result.fallbackMessage ?? null };
			}
			default: throw new Error(`unknown operation ${request.operation}`);
		}
	});
}
void main();
