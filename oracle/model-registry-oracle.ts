// Frozen registry and its default Agent dispatch consumer; no live provider requests.

import { existsSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

function createStreamRejection(
	kind: unknown,
	value: unknown,
	method: unknown,
): unknown {
	if (kind === "error") {
		return Object.assign(new TypeError(value as string), {
			code: "E_STREAM",
			detail: { method },
		});
	}
	if (kind === "conversion") return { toString: null, valueOf: null };
	return value;
}

async function main(): Promise<void> {
	installScriptedFetch();
	const source = pathToFileURL(process.argv[2]);
	const { ModelRegistry, clearApiKeyCache } = await import(source.href);
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
		if (request.operation === "constructor_lifetime") {
			if (typeof request.home !== "string")
				throw new TypeError("Fixture home must be a string");
			process.env.HOME = request.home;
			if (typeof request.agentDir === "string")
				process.env.PI_CODING_AGENT_DIR = request.agentDir;
			else delete process.env.PI_CODING_AGENT_DIR;
			try {
				const registry = ModelRegistry.create(
					AuthStorage.inMemory({}),
					request.path,
				);
				const snapshot = () => {
					const models = registry
						.getAll()
						.filter(
							(model: { provider: string }) =>
								model.provider === "constructor-fixture",
						);
					return {
						error: registry.getError() ?? null,
						models,
						serializedModels: JSON.stringify(models),
					};
				};
				const before = snapshot();
				if (typeof request.nextHome === "string")
					process.env.HOME = request.nextHome;
				registry.refresh();
				return { created: true, before, after: snapshot() };
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				return {
					created: false,
					error: {
						name: error.name,
						message: error.message,
						...Object.fromEntries(
							Object.entries(error).filter(([key]) => key !== "stack"),
						),
					},
				};
			}
		}
		const storage = AuthStorage.inMemory(request.auth ?? {});
		const registry = request.modelsPath
			? ModelRegistry.create(storage, request.modelsPath)
			: ModelRegistry.inMemory(storage);
		if (request.operation === "modifier_throw") {
			const provider = "registry-modifier-throw";
			const credential = {
				type: "oauth",
				access: "access",
				refresh: "refresh",
				expires: 9999999999999,
			};
			const thrown =
				request.throwKind === "error"
					? Object.assign(new TypeError(request.value as string), {
							code: "E_MODIFIER",
							detail: { stage: request.stage },
						})
					: request.value;
			if (request.stage === "register") storage.set(provider, credential);
			let rejection: unknown = null;
			try {
				registry.registerProvider(provider, {
					...(request.config as Record<string, unknown>),
					name: "Registered Modifier",
					oauth: {
						name: "Modifier Fixture",
						login: async () => {
							throw new Error("unused login");
						},
						refreshToken: async (value: unknown) => value,
						getApiKey: (value: { access: string }) => value.access,
						modifyModels: () => {
							throw thrown;
						},
					},
				});
				if (request.stage !== "register") {
					storage.set(provider, credential);
					if (request.stage === "unregister") {
						registry.registerProvider("modifier-side", { name: "Side" });
						registry.unregisterProvider("modifier-side");
					} else if (request.stage === "refresh") registry.refresh();
					else if (request.stage === "create")
						ModelRegistry.create(storage, "");
					else ModelRegistry.inMemory(storage);
				}
			} catch (error) {
				rejection =
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
			}
			return {
				rejection,
				ids: registry
					.getAll()
					.filter((model: { provider: string }) => model.provider === provider)
					.map((model: { id: string }) => model.id),
				display: registry.getProviderDisplayName(provider),
				sideDisplay: registry.getProviderDisplayName("modifier-side"),
			};
		}
		if (request.operation === "registration_reentry") {
			const provider = "registry-reentrant";
			const nested = "registry-reentrant-nested";
			const observations: unknown[] = [];
			const snapshot = () => ({
				ids: registry
					.getAll()
					.filter((model: { provider: string }) => model.provider === provider)
					.map((model: { id: string }) => model.id),
				display: registry.getProviderDisplayName(provider),
				nestedDisplay: registry.getProviderDisplayName(nested),
			});
			registry.registerProvider(provider, {
				...(request.config as Record<string, unknown>),
				name: "Registered Fixture",
				oauth: {
					name: "OAuth Fixture",
					login: async () => {
						throw new Error("unused login");
					},
					refreshToken: async (credential: unknown) => credential,
					getApiKey: (credential: { access: string }) => credential.access,
					modifyModels: (models: unknown[], credential: { access: string }) => {
						observations.push({ ...snapshot(), access: credential.access });
						registry.registerProvider(nested, {
							name: `Nested ${observations.length}`,
						});
						return models;
					},
				},
			});
			registry.refresh();
			const afterRefresh = snapshot();
			registry.unregisterProvider(provider);
			const afterUnregister = snapshot();
			registry.unregisterProvider(nested);
			return { observations, afterRefresh, afterUnregister };
		}
		if (request.operation === "auth_reentry") {
			const provider = "registry-auth-reentrant";
			let count = 0;
			const definition = (
				request.config as { models: Record<string, unknown>[] }
			).models[0];
			registry.registerProvider(provider, {
				...(request.config as Record<string, unknown>),
				headers: { "X-Provider": "provider-0\uDC00", "X-Same": "provider-0" },
				models: [
					{
						...definition,
						headers: { "X-Model": "model-0\uD800", "X-Same": "model-0" },
					},
				],
				oauth: {
					name: "OAuth Header Fixture",
					login: async () => {
						throw new Error("unused login");
					},
					refreshToken: async (credential: unknown) => credential,
					getApiKey: (credential: { access: string }) => {
						count++;
						registry.registerProvider(provider, {
							...(request.config as Record<string, unknown>),
							apiKey: "updated-provider-key",
							authHeader: true,
							headers: {
								"X-Provider": `provider-${count}\uDC00`,
								"X-Same": `provider-${count}`,
							},
							models: [
								{
									...definition,
									headers: {
										"X-Model": `model-${count}\uD800`,
										"X-Same": `model-${count}`,
									},
								},
							],
						});
						return credential.access;
					},
				},
			});
			const model = registry.find(provider, "one");
			model.headers = {
				"X-Static": "static",
				"X-Same": "static",
				Authorization: "static-auth",
			};
			const first = await registry.getApiKeyAndHeaders(model);
			const second = await registry.getApiKeyAndHeaders(model);
			registry.unregisterProvider(provider);
			return { first, second, count };
		}
		if (request.operation === "provider_auth_snapshot") {
			const provider = "registry-provider-auth";
			delete process.env.PI_REGISTRY_PROVIDER_KEY;
			delete process.env.PI_REGISTRY_PROVIDER_ABSENT;
			if (typeof request.envValue === "string")
				process.env.PI_REGISTRY_PROVIDER_KEY = request.envValue;
			if (request.config) registry.registerProvider(provider, request.config);
			if (typeof request.runtimeKey === "string")
				storage.setRuntimeApiKey(provider, request.runtimeKey);
			const model = {
				...registry.find("anthropic", "claude-opus-4-8"),
				provider,
			};
			const witness =
				typeof request.witness === "string" ? request.witness : undefined;
			const readCommandCount = () =>
				witness && existsSync(witness)
					? readFileSync(witness, "utf8").length
					: 0;
			const before = {
				configured: registry.hasConfiguredAuth(model),
				status: registry.getProviderAuthStatus(provider),
				display: registry.getProviderDisplayName(provider),
				oauth: registry.isUsingOAuth(model),
				commandCount: readCommandCount(),
			};
			const key = await registry.getApiKeyForProvider(provider);
			const auth = await registry.getApiKeyAndHeaders(model);
			return {
				before,
				key: key ?? null,
				auth,
				commandCount: readCommandCount(),
			};
		}
		if (request.operation === "registration_live_map") {
			const provider = "registry-live-parent";
			const side = "registry-live-side";
			const config = request.config as { models: Record<string, unknown>[] };
			const sideConfig = (name: string) => ({
				...config,
				apiKey: "side-key",
				name,
				models: [{ ...config.models[0], id: "side", name }],
			});
			let calls = 0;
			registry.registerProvider(provider, {
				...config,
				oauth: {
					name: "Live Parent",
					login: async () => {
						throw new Error("unused login");
					},
					refreshToken: async (value: unknown) => value,
					getApiKey: (value: { access: string }) => value.access,
					modifyModels: (models: unknown[]) => {
						calls++;
						if (calls === 1) {
							if (request.mode !== "append") registry.unregisterProvider(side);
							if (request.mode !== "delete")
								registry.registerProvider(side, sideConfig("Side new"));
						}
						return models;
					},
				},
			});
			if (request.mode !== "append")
				registry.registerProvider(side, sideConfig("Side old"));
			storage.set(provider, {
				type: "oauth",
				access: "access",
				refresh: "refresh",
				expires: 9999999999999,
			});
			const snapshot = () => {
				const models = registry
					.getAll()
					.filter(
						(model: { provider: string }) =>
							model.provider === provider || model.provider === side,
					);
				return {
					models,
					serializedModels: JSON.stringify(models),
					sideDisplay: registry.getProviderDisplayName(side),
				};
			};
			registry.refresh();
			const first = snapshot();
			registry.refresh();
			const second = snapshot();
			registry.unregisterProvider(provider);
			registry.unregisterProvider(side);
			return { mode: request.mode, first, second, calls };
		}
		if (request.operation === "registration_stream_throw") {
			const provider = "registry-stream-throw";
			const api = "registry-stream-throw-api";
			const thrown = createStreamRejection(
				request.kind,
				request.value,
				request.method,
			);
			let callbackCount = 0;
			registry.registerProvider(provider, {
				...(request.config as Record<string, unknown>),
				api,
				apiKey: "config-key",
				streamSimple: () => {
					callbackCount++;
					throw thrown;
				},
			});
			const model = registry.find(provider, "one");
			const dispatch = await import(
				new URL("../../../ai/src/compat.ts", source).href
			);
			let rejection: unknown = null;
			try {
				dispatch[request.method as "stream" | "streamSimple"](
					model,
					{ messages: [] },
					{ apiKey: "stream-key" },
				);
			} catch (error) {
				rejection =
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
			}
			registry.unregisterProvider(provider);
			return { callbackCount, rejection };
		}
		if (request.operation === "registration_agent_throw") {
			const provider = "registry-agent-throw";
			let callbackCount = 0;
			const thrown = createStreamRejection(
				request.kind,
				request.value,
				"streamSimple",
			);
			registry.registerProvider(provider, {
				...(request.config as Record<string, unknown>),
				api: "registry-agent-throw-api",
				apiKey: "config-key",
				streamSimple: () => {
					callbackCount++;
					throw thrown;
				},
			});
			const { Agent } = await import(
				new URL("../../../agent-core/src/agent.ts", source).href
			);
			const agent = new Agent({
				initialState: { model: registry.find(provider, "one") },
				getApiKey: async () => "stream-key",
			});
			const events: unknown[] = [];
			agent.subscribe((event: unknown) => {
				events.push(event);
			});
			const before = Date.now();
			let result: unknown = { kind: "fulfilled" };
			try {
				await agent.prompt("prompt");
			} catch (error) {
				result =
					error instanceof Error
						? {
								kind: "rejected",
								value: { name: error.name, message: error.message },
							}
						: { kind: "rejected", value: error };
			}
			const state = agent.state;
			registry.unregisterProvider(provider);
			return {
				clock: { before, after: Date.now() },
				snapshot: {
					callbackCount,
					result,
					events,
					state: {
						messages: state.messages,
						isStreaming: state.isStreaming,
						isModelRequestActive: state.isModelRequestActive,
						streamingMessage: state.streamingMessage ?? null,
						pendingToolCalls: [...state.pendingToolCalls],
						errorMessage: state.errorMessage ?? null,
					},
				},
			};
		}
		if (request.operation === "clear_alias_snapshot") {
			if (
				typeof request.command !== "string" ||
				typeof request.witness !== "string"
			)
				throw new TypeError("Fixture command/witness must be strings");
			const config = await import(
				new URL("resolve-config-value.ts", source).href
			);
			const first = config.resolveConfigValue(request.command);
			const second = config.resolveConfigValue(request.command);
			const before = readFileSync(request.witness, "utf8").length;
			clearApiKeyCache();
			const third = config.resolveConfigValue(request.command);
			return {
				values: [first, second, third],
				before,
				after: readFileSync(request.witness, "utf8").length,
			};
		}
		if (request.operation === "config_request") {
			const response = request.response;
			const modelId = request.modelId;
			if (typeof modelId !== "string")
				throw new TypeError("Fixture model ID must be a string");
			if (typeof response !== "string")
				throw new TypeError("Fixture response must be a string");
			const requests: unknown[] = [];
			setFetchScript((async (input: unknown, init?: RequestInit) => {
				const requestObject = input instanceof Request ? input : undefined;
				const headers = new Headers(init?.headers ?? requestObject?.headers);
				const body = requestObject
					? await requestObject.text()
					: String(init?.body);
				requests.push({
					body: JSON.parse(body),
					headers: Object.fromEntries(
						["session_id", "x-client-request-id", "x-session-affinity"].map(
							(name) => [name, headers.get(name)],
						),
					),
				});
				return new Response(response, {
					status: 200,
					headers: { "content-type": "text/event-stream" },
				});
			}) as typeof fetch);
			try {
				const model = registry.find("local", modelId);
				const auth = await registry.getApiKeyAndHeaders(model);
				if (!auth.ok) throw new Error(auth.error);
				const api = await import(
					new URL(`../../../ai/src/api/${request.api}.ts`, source).href
				);
				const stream = api[request.simple ? "streamSimple" : "stream"](
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
						...(request.simple ? { reasoning: "high" } : {}),
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
		if (
			request.operation === "registration_snapshot" ||
			request.operation === "registration_from_existing"
		) {
			let config = request.config;
			if (request.operation === "registration_from_existing") {
				const model = registry.find(
					request.sourceProvider,
					request.sourceModel,
				);
				if (!model) throw new Error("Source fixture model missing");
				config = { ...(config as Record<string, unknown>), models: [model] };
			}
			const updates = request.updates;
			if (updates !== undefined && !Array.isArray(updates))
				throw new TypeError("Fixture updates must be an array");
			let error = null;
			try {
				registry.registerProvider(request.provider, config);
				for (const update of updates ?? [])
					registry.registerProvider(request.provider, update);
			} catch (failure) {
				error = failure instanceof Error ? failure.message : String(failure);
			}
			const models = registry
				.getAll()
				.filter(
					(model: { provider: string }) => model.provider === request.provider,
				);
			const snapshot = {
				error,
				models,
				serializedModels: JSON.stringify(models),
			};
			if (request.operation === "registration_from_existing") {
				const value = models[0]?.compat?.supportsStore;
				return {
					...snapshot,
					numberState: {
						type: typeof value,
						finite: Number.isFinite(value),
						positiveInfinity: value === Infinity,
					},
				};
			}
			return snapshot;
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
