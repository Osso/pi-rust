import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Model = {
	id: string;
	provider: string;
	[key: string]: unknown;
};

type Runtime = {
	getProviders(): Array<{
		id: string;
		name: string;
		auth: {
			apiKey?: { name: string };
			oauth?: { name: string; isSubscription?: boolean };
		};
	}>;
	getModels(provider?: string): Model[];
	getError(): string | undefined;
	getProviderAuthStatus(provider: string): { configured: boolean; source?: string; label?: string };
	getAvailableSnapshot(): Model[];
	getRegisteredProviderIds(): string[];
	getAuth(provider: string): Promise<
		| {
				auth: { apiKey?: string; headers?: Record<string, string | null>; baseUrl?: string };
				env?: Record<string, string>;
				source?: string;
		  }
		| undefined
	>;
	getCompatibilityRequestConfig(model: Model): { headers?: Record<string, string | null>; authHeader: boolean };
};

type RuntimeApi = {
	ModelRuntime: {
		create(options: {
			credentials: unknown;
			modelsPath: string | null;
		}): Promise<Runtime>;
	};
};

type AuthStorageApi = {
	AuthStorage: {
		inMemory(data: Record<string, unknown>): unknown;
	};
};

type Request = {
	id: RequestId;
	modelsPath: string;
	auth: Record<string, unknown>;
	authProviders: string[];
	modelRefs: Array<[string, string]>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: RuntimeApi, authApi: AuthStorageApi): Promise<unknown> {
	const runtime = await api.ModelRuntime.create({
		credentials: authApi.AuthStorage.inMemory(request.auth),
		modelsPath: request.modelsPath,
	});
	const providers = runtime.getProviders();
	const models = runtime.getModels();
	const authStatuses = providers.map((provider) => {
		const status = runtime.getProviderAuthStatus(provider.id);
		return {
			provider: provider.id,
			configured: status.configured,
			source: status.source ?? null,
			label: status.label ?? null,
		};
	});
	const auth: Record<string, unknown> = {};
	for (const provider of request.authProviders) {
		const resolution = await runtime.getAuth(provider);
		auth[provider] = resolution
			? {
					apiKey: resolution.auth.apiKey ?? null,
					headers: resolution.auth.headers ?? null,
					baseUrl: resolution.auth.baseUrl ?? null,
					env: resolution.env ?? null,
					source: resolution.source ?? null,
				}
			: null;
	}
	const compatibility: Record<string, unknown> = {};
	for (const [provider, id] of request.modelRefs) {
		const model = models.find((candidate) => candidate.provider === provider && candidate.id === id);
		if (!model) {
			compatibility[`${provider}/${id}`] = null;
			continue;
		}
		const config = runtime.getCompatibilityRequestConfig(model);
		compatibility[`${provider}/${id}`] = {
			headers: config.headers ?? null,
			authHeader: config.authHeader,
		};
	}
	return {
		providers: providers.map((provider) => ({
			id: provider.id,
			name: provider.name,
			apiKeyName: provider.auth.apiKey?.name ?? null,
			oauthName: provider.auth.oauth?.name ?? null,
			oauthSubscription: provider.auth.oauth?.isSubscription === true,
		})),
		models,
		error: runtime.getError() ?? null,
		authStatuses,
		available: runtime.getAvailableSnapshot().map((model) => ({ provider: model.provider, id: model.id })),
		registeredProviderIds: runtime.getRegisteredProviderIds(),
		auth,
		compatibility,
	};
}

function serve(api: RuntimeApi, authApi: AuthStorageApi): void {
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
					const output = await execute(request, api, authApi);
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
	if (!sourceModulePath) throw new Error("model runtime oracle requires a source module path");
	const sourceUrl = pathToFileURL(sourceModulePath);
	const api = (await import(sourceUrl.href)) as RuntimeApi;
	const authModuleUrl = new URL("auth-storage.ts", sourceUrl);
	const authApi = (await import(authModuleUrl.href)) as AuthStorageApi;
	serve(api, authApi);
}

void main().catch((error: unknown) => {
	process.stderr.write(`model runtime oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
