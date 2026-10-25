import { pathToFileURL } from "node:url";
import type {
	Context,
	Model,
	SimpleStreamOptions,
} from "@pi-oracle-source/ai/src/types.ts";
import type { ProviderConfigInput } from "@pi-oracle-source/coding-agent/src/core/model-registry.ts";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	let fetchCount = 0;
	globalThis.fetch = async () => {
		fetchCount++;
		throw new Error("Fixture must not issue HTTP");
	};
	const source = pathToFileURL(process.argv[2]);
	const { ModelRegistry } = await import(source.href);
	const { AuthStorage } = await import(new URL("auth-storage.ts", source).href);
	const compat = await import(
		new URL("../../../ai/src/compat.ts", source).href
	);
	serve(async () => {
		const registry = ModelRegistry.inMemory(AuthStorage.inMemory({}));
		const outside = ModelRegistry.inMemory(AuthStorage.inMemory({}));
		const provider = "registry-dispatch-lifecycle";
		const outsideName = "registry-dispatch-outside";
		const calls: unknown[] = [];
		const oauth = (name: string) => ({
			name,
			login: async () => {
				throw new Error("unused login");
			},
			refreshToken: async () => {
				throw new Error("unused refresh");
			},
			getApiKey: () => {
				throw new Error("unused OAuth key");
			},
		});
		const callback =
			(label: string) =>
			(model: Model, context: Context, options?: SimpleStreamOptions) => {
				calls.push({
					label,
					model,
					context,
					options: {
						apiKey: options?.apiKey,
						temperature: options?.temperature,
						maxTokens: options?.maxTokens,
						headers: options?.headers,
						reasoning: options?.reasoning ?? null,
					},
				});
				const message = {
					role: "assistant",
					content: [{ type: "text", text: label }],
					api: model.api,
					provider: model.provider,
					model: model.id,
					usage: {
						input: 0,
						output: 0,
						cacheRead: 0,
						cacheWrite: 0,
						totalTokens: 0,
						cost: {
							input: 0,
							output: 0,
							cacheRead: 0,
							cacheWrite: 0,
							total: 0,
						},
					},
					stopReason: "stop",
					timestamp: 0,
				};
				const stream = compat.createAssistantMessageEventStream();
				stream.push({ type: "start", partial: message });
				stream.push({ type: "done", reason: "stop", message });
				stream.end(message);
				return stream;
			};
		outside.registerProvider(outsideName, {
			api: "registry-outside-api",
			streamSimple: callback("outside"),
			oauth: oauth("Outside"),
		});
		const outsidePresent = () => ({
			api: !!compat.getApiProvider("registry-outside-api"),
			oauth: registry.authStorage
				.getOAuthProviders()
				.some((item: { id: string }) => item.id === outsideName),
		});
		registry.unregisterProvider("never-registered");
		const unknownNoop = outsidePresent();
		const definition = {
			id: "one",
			name: "One",
			reasoning: false,
			input: ["text"],
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
			contextWindow: 128,
			maxTokens: 32,
		};
		const config = {
			api: "openai-responses",
			baseUrl: "https://fixture.invalid",
			apiKey: "config-key",
			headers: { "X-Phase": "first" },
			models: [definition],
			streamSimple: callback("first"),
			oauth: oauth("OAuth First"),
		} as ProviderConfigInput;
		registry.registerProvider(provider, config);
		const model = registry.find(provider, "one");
		const snapshot = async (phase: string) => ({
			phase,
			models: registry
				.getAll()
				.filter((item: Model) => item.provider === provider),
			status: registry.getProviderAuthStatus(provider),
			display: registry.getProviderDisplayName(provider),
			key: (await registry.getApiKeyForProvider(provider)) ?? null,
			requestAuth: await registry.getApiKeyAndHeaders(model),
			configured: registry.hasConfiguredAuth(model),
			outside: outsidePresent(),
			oauthName:
				registry.authStorage
					.getOAuthProviders()
					.find((item: { id: string }) => item.id === provider)?.name ?? null,
		});
		const states = [await snapshot("registered")];
		const context: Context = {
			systemPrompt: "",
			messages: [{ role: "user", content: "request-\ud800", timestamp: 1 }],
		};
		const dispatch = async (method: "stream" | "streamSimple") => {
			const options: SimpleStreamOptions = {
				apiKey: "wire-key",
				temperature: 0.5,
				maxTokens: 7,
				headers: { "X-Call": "caller-\ud800" },
			};
			if (method === "streamSimple") options.reasoning = "high";
			const stream = compat[method](model, context, options);
			const events: unknown[] = [];
			for await (const event of stream) events.push(event);
			return { events, result: await stream.result() };
		};
		const dispatches = [
			await dispatch("stream"),
			await dispatch("streamSimple"),
		];
		registry.registerProvider(provider, {
			api: "openai-responses",
			streamSimple: callback("second"),
			headers: { "X-Phase": "second" },
		});
		states.push(await snapshot("partial-upsert"));
		dispatches.push(await dispatch("stream"), await dispatch("streamSimple"));
		registry.registerProvider(provider, { oauth: oauth("OAuth Second") });
		states.push(await snapshot("oauth-upsert"));
		registry.refresh();
		states.push(await snapshot("refreshed"));
		dispatches.push(await dispatch("stream"), await dispatch("streamSimple"));
		registry.unregisterProvider(provider);
		states.push(await snapshot("unregistered"));
		// No key and an unknown provider prevent the restored built-in from making HTTP requests.
		const restored = compat.streamSimple(model, context, { env: {} });
		const restoredResult = await restored.result();
		const restoredSummary = {
			stopReason: restoredResult.stopReason,
			errorMessage: restoredResult.errorMessage,
		};
		registry.unregisterProvider(provider);
		states.push(await snapshot("unregistered-again"));
		return {
			unknownNoop,
			states,
			dispatches,
			calls,
			restoredSummary,
			fetchCount,
		};
	});
}
main().catch((error) => {
	console.error(error);
	process.exit(1);
});
