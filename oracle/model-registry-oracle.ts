// Frozen registry; no runtime/composer modules or live provider requests.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

async function main(): Promise<void> {
	installScriptedFetch();
	const source = pathToFileURL(process.argv[2]);
	const { ModelRegistry } = await import(source.href);
	const { AuthStorage } = await import(new URL("auth-storage.ts", source).href);
	serve(async (request) => {
		if (request.operation === "oauth_throw") {
			const provider = "q15-oauth-throw";
			const storage = AuthStorage.inMemory({
				[provider]: {
					type: "oauth",
					access: "access",
					refresh: "refresh",
					expires: request.expired ? 0 : 9999999999999,
				},
			});
			const registry = ModelRegistry.inMemory(storage);
			const rejection =
				request.throwKind === "error"
					? Object.assign(new TypeError(request.value as string), {
							code: "E_Q15",
							detail: request.detail,
						})
					: request.value;
			registry.registerProvider(provider, {
				oauth: {
					name: "Q15 throw fixture",
					login: async () => {
						throw new Error("unused login");
					},
					refreshToken: async (credential: unknown) => ({
						...(credential as Record<string, unknown>),
						expires: 9999999999999,
					}),
					getApiKey: () => {
						throw rejection;
					},
				},
			});
			const snapshot = (error: unknown) =>
				error instanceof Error
					? {
							kind: "error",
							value: {
								name: error.name,
								message: error.message,
								...Object.fromEntries(
									Object.entries(error).filter(([key]) => key !== "stack"),
								),
							},
						}
					: { kind: "value", value: error };
			let key: unknown;
			try {
				key = {
					ok: true,
					value: await registry.getApiKeyForProvider(provider),
				};
			} catch (error) {
				key = { ok: false, rejection: snapshot(error) };
			}
			let auth: unknown;
			const model = {
				...registry.find("anthropic", "claude-opus-4-8"),
				provider,
			};
			try {
				auth = { ok: true, value: await registry.getApiKeyAndHeaders(model) };
			} catch (error) {
				auth = { ok: false, rejection: snapshot(error) };
			}
			return { key, auth };
		}
		const storage = AuthStorage.inMemory(request.auth ?? {});
		const registry = request.modelsPath
			? ModelRegistry.create(storage, request.modelsPath)
			: ModelRegistry.inMemory(storage);
		if (request.operation === "config_request") {
			const requests: unknown[] = [];
			setFetchScript((async (_input: unknown, init?: RequestInit) => {
				const headers = new Headers(init?.headers);
				requests.push({
					body: JSON.parse(String(init?.body)),
					headers: Object.fromEntries(
						["session_id", "x-client-request-id", "x-session-affinity"].map(
							(name) => [name, headers.get(name)],
						),
					),
				});
				return new Response(
					`data: ${JSON.stringify({ id: "fixture", object: "chat.completion.chunk", created: 1, model: "fixture", choices: [{ index: 0, delta: { content: "Hello" }, finish_reason: request.finishReason === false ? null : "stop" }] })}\n\ndata: [DONE]\n\n`,
					{ status: 200, headers: { "content-type": "text/event-stream" } },
				);
			}) as typeof fetch);
			try {
				const model = registry.find("local", "fixture-model");
				const auth = await registry.getApiKeyAndHeaders(model);
				if (!auth.ok) throw new Error(auth.error);
				const api = await import(
					new URL("../../../ai/src/api/openai-completions.ts", source).href
				);
				const stream = api.stream(
					model,
					{
						systemPrompt: "fixture system",
						messages: request.messages ?? [
							{ role: "user", content: "Hello", timestamp: 1 },
						],
						tools: request.tools,
					},
					{
						apiKey: auth.apiKey,
						headers: auth.headers,
						maxTokens: 11,
						...(request.streamOptions as Record<string, unknown> | undefined),
					},
				);
				for await (const _event of stream) {
				}
				const result = await stream.result();
				return {
					error: registry.getError() ?? null,
					requests,
					errorMessage: result.errorMessage ?? null,
					stopReason: result.stopReason,
				};
			} finally {
				clearFetchScript();
			}
		}
		if (request.operation === "config_snapshot") {
			return {
				error: registry.getError() ?? null,
				models: registry.getAll(),
				serializedModels: JSON.stringify(registry.getAll()),
			};
		}
		if (request.operation === "register_custom_auth") {
			const provider = "ordered-custom";
			const model = {
				...registry.find("anthropic", "claude-opus-4-8"),
				provider,
			};
			let error = null;
			try {
				registry.registerProvider(provider, {
					api: model.api,
					apiKey: "private-fixture-key",
					models: [model],
					...(request.baseUrl ? { baseUrl: model.baseUrl } : {}),
				});
			} catch (failure) {
				error = failure instanceof Error ? failure.message : String(failure);
			}
			return {
				error,
				configured: registry.hasConfiguredAuth(model),
				auth: await registry.getApiKeyAndHeaders(model),
			};
		}
		if (request.operation === "resolve_config") {
			const model = registry.find(request.provider, request.model);
			const auth = await registry.getApiKeyAndHeaders(model);
			if (!auth.ok) throw new Error(auth.error);
			return {
				error: registry.getError() ?? null,
				model,
				key: await registry.getApiKeyForProvider(request.provider),
				request: {
					baseUrl: model.baseUrl,
					apiKey: auth.apiKey,
					headers: auth.headers,
				},
			};
		}
		const model = registry.find("anthropic", "claude-opus-4-8");
		const snapshot = () => ({
			configured: registry.hasConfiguredAuth(model),
			available: registry
				.getAvailable()
				.some(
					(item: { provider: string; id: string }) =>
						item.provider === model.provider && item.id === model.id,
				),
			oauth: registry.isUsingOAuth(model),
		});
		const before = snapshot();
		storage.set("anthropic", { type: "api_key", key: "q15-key" });
		const after = snapshot();
		storage.set("anthropic", {
			type: "oauth",
			access: "q15-access",
			refresh: "q15-refresh",
			expires: 9999999999999,
		});
		const oauth = snapshot();
		storage.remove("anthropic");
		return { before, after, oauth, removed: snapshot() };
	});
}
void main();
