import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Model = {
	id: string;
	provider: string;
	name: string;
	baseUrl: string;
	reasoning: boolean;
	[key: string]: unknown;
};

type ModelResolverApi = {
	defaultModelPerProvider: Record<string, string>;
	findExactModelReferenceMatch(reference: string, models: readonly Model[]): Model | undefined;
	parseModelPattern(
		pattern: string,
		models: readonly Model[],
		options?: { allowInvalidThinkingLevelFallback?: boolean },
	): { model: Model | undefined; thinkingLevel?: string; warning: string | undefined };
	resolveModelScopeFromModels(
		patterns: string[],
		models: readonly Model[],
	): {
		scopedModels: Array<{ model: Model; thinkingLevel?: string }>;
		diagnostics: Array<{ type: string; code: string; message: string; pattern: string }>;
	};
	resolveCliModel(options: {
		cliProvider?: string;
		cliModel?: string;
		cliThinking?: string;
		modelRuntime: unknown;
	}): { model: Model | undefined; thinkingLevel?: string; warning: string | undefined; error: string | undefined };
	findInitialModel(options: {
		cliProvider?: string;
		cliModel?: string;
		scopedModels: Array<{ model: Model; thinkingLevel?: string }>;
		isContinuing: boolean;
		defaultProvider?: string;
		defaultModelId?: string;
		defaultThinkingLevel?: string;
		modelRuntime: unknown;
	}): Promise<{
		model: Model | undefined;
		thinkingLevel: string;
		fallbackMessage: string | undefined;
	}>;
};

type Request =
	| { id: RequestId; operation: "defaults" }
	| { id: RequestId; operation: "exact"; reference: string; models: Model[] }
	| {
			id: RequestId;
			operation: "parse";
			pattern: string;
			models: Model[];
			allowInvalidThinkingLevelFallback?: boolean;
	  }
	| { id: RequestId; operation: "scope"; patterns: string[]; models: Model[] }
	| {
			id: RequestId;
			operation: "cli";
			models: Model[];
			authenticated: string[];
			cliProvider?: string;
			cliModel?: string;
			cliThinking?: string;
	  }
	| {
			id: RequestId;
			operation: "initial";
			models: Model[];
			available: Model[];
			authenticated: string[];
			cliProvider?: string;
			cliModel?: string;
			scopedModels: Array<{ model: Model; thinkingLevel?: string }>;
			isContinuing: boolean;
			defaultProvider?: string;
			defaultModelId?: string;
			defaultThinkingLevel?: string;
	  };

function normalizeModel(model: Model | undefined): Record<string, unknown> | null {
	if (!model) return null;
	return {
		id: model.id,
		provider: model.provider,
		name: model.name,
		baseUrl: model.baseUrl,
		reasoning: model.reasoning,
	};
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function cliRuntime(models: Model[], authenticated: string[]) {
	return {
		getModels: () => models,
		hasConfiguredAuth: (provider: string) => authenticated.includes(provider),
	};
}

async function execute(request: Request, api: ModelResolverApi): Promise<unknown> {
	switch (request.operation) {
		case "defaults":
			return { defaults: api.defaultModelPerProvider };
		case "exact":
			return { model: normalizeModel(api.findExactModelReferenceMatch(request.reference, request.models)) };
		case "parse": {
			const result = api.parseModelPattern(
				request.pattern,
				request.models,
				request.allowInvalidThinkingLevelFallback === undefined
					? undefined
					: { allowInvalidThinkingLevelFallback: request.allowInvalidThinkingLevelFallback },
			);
			return {
				model: normalizeModel(result.model),
				thinkingLevel: result.thinkingLevel ?? null,
				warning: result.warning ?? null,
			};
		}
		case "scope": {
			const result = api.resolveModelScopeFromModels(request.patterns, request.models);
			return {
				scopedModels: result.scopedModels.map((scoped) => ({
					model: normalizeModel(scoped.model),
					thinkingLevel: scoped.thinkingLevel ?? null,
				})),
				diagnostics: result.diagnostics.map((diagnostic) => ({
					code: diagnostic.code,
					message: diagnostic.message,
					pattern: diagnostic.pattern,
				})),
			};
		}
		case "cli": {
			const result = api.resolveCliModel({
				cliProvider: request.cliProvider,
				cliModel: request.cliModel,
				cliThinking: request.cliThinking as never,
				modelRuntime: cliRuntime(request.models, request.authenticated),
			});
			return {
				model: normalizeModel(result.model),
				thinkingLevel: result.thinkingLevel ?? null,
				warning: result.warning ?? null,
				error: result.error ?? null,
			};
		}
		case "initial": {
			const modelsById = new Map(request.models.map((model) => [`${model.provider}/${model.id}`, model]));
			const runtime = {
				getModels: () => request.models,
				getAvailableSnapshot: () => request.available,
				getModel: (provider: string, modelId: string) => modelsById.get(`${provider}/${modelId}`),
				hasConfiguredAuth: (provider: string) => request.authenticated.includes(provider),
			};
			const result = await api.findInitialModel({
				cliProvider: request.cliProvider,
				cliModel: request.cliModel,
				scopedModels: request.scopedModels,
				isContinuing: request.isContinuing,
				defaultProvider: request.defaultProvider,
				defaultModelId: request.defaultModelId,
				defaultThinkingLevel: request.defaultThinkingLevel as never,
				modelRuntime: runtime,
			});
			return {
				model: normalizeModel(result.model),
				thinkingLevel: result.thinkingLevel,
				fallbackMessage: result.fallbackMessage ?? null,
			};
		}
		default:
			throw new Error(`unknown operation ${(request as { operation: string }).operation}`);
	}
}

function serve(api: ModelResolverApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			let id: RequestId = null;
			void (async () => {
				try {
					const request = JSON.parse(line) as Request;
					id = request.id;
					const output = await execute(request, api);
					process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
				} catch (error) {
					process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
				}
			})();
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (!sourceModulePath) throw new Error("model resolver oracle requires a source module path");
	const api = (await import(pathToFileURL(sourceModulePath).href)) as ModelResolverApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`model resolver oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
