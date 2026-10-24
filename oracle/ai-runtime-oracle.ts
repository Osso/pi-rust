import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function normalizeMessage(
	message: Record<string, unknown>,
): Record<string, unknown> {
	const { timestamp: _timestamp, ...rest } = message;
	return rest;
}

function testModel(provider: string, id: string): Record<string, unknown> {
	return {
		id,
		name: id,
		api: "test-api",
		provider,
		baseUrl: "https://example.test/v1",
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 10000,
		maxTokens: 1000,
	};
}

function doneMessage(
	model: Record<string, unknown>,
	text: string,
): Record<string, unknown> {
	return {
		role: "assistant",
		content: [{ type: "text", text }],
		api: model.api,
		provider: model.provider,
		model: model.id,
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "stop",
		timestamp: Date.now(),
	};
}

async function collect(stream: AsyncIterable<unknown>): Promise<unknown[]> {
	const events: unknown[] = [];
	for await (const event of stream) events.push(event);
	return events;
}

async function executeAsync(
	request: any,
	api: Record<string, any>,
): Promise<Response> {
	const {
		calculateCost,
		getSupportedThinkingLevels,
		clampThinkingLevel,
		createModels,
		createProvider,
	} = api.models;
	if (request.operation === "cost") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item: any) => {
					const usage = {
						...item.usage,
						cost: {
							input: 0,
							output: 0,
							cacheRead: 0,
							cacheWrite: 0,
							total: 0,
						},
					};
					return calculateCost(item.model, usage);
				}),
			},
		};
	}
	if (request.operation === "thinking") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item: any) => ({
					supported: getSupportedThinkingLevels(item.model),
					clamped: item.levels.map((level: string) =>
						clampThinkingLevel(item.model, level),
					),
				})),
			},
		};
	}
	if (request.operation === "equality") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item: { a: unknown; b: unknown }) =>
					api.models.modelsAreEqual(item.a, item.b),
				),
			},
		};
	}
	if (request.operation === "auth_resolve") {
		const { InMemoryCredentialStore } = api.credentialStore;
		const { envApiKeyAuth } = api.helpers;
		const { resolveProviderAuth } = api.resolve;
		const results = [];
		for (const item of request.cases) {
			const credentials = new InMemoryCredentialStore();
			for (const credential of item.credentials ?? []) {
				await credentials.modify(item.providerId, async () => credential);
			}
			const ctx = {
				env: async (name: string) => item.env?.[name],
				fileExists: async () => false,
			};
			const provider = {
				id: item.providerId,
				auth: {
					apiKey: item.apiKeyEnv
						? envApiKeyAuth("Test key", item.apiKeyEnv)
						: undefined,
					oauth: item.oauth
						? {
								name: "Test OAuth",
								login: async () => {
									throw new Error("not used");
								},
								refresh: async (credential: any) => credential,
								toAuth: async (credential: any) => ({
									apiKey: credential.access,
								}),
							}
						: undefined,
				},
			};
			try {
				const result = await resolveProviderAuth(
					provider,
					testModel(item.providerId, "auth-model"),
					credentials,
					ctx,
					item.overrides,
				);
				results.push({ name: item.name, ok: true, result });
			} catch (error) {
				const normalized = normalizeError(error);
				results.push({
					name: item.name,
					ok: false,
					error: {
						name: normalized.name,
						code: (error as any).code,
						message: normalized.message,
					},
				});
			}
		}
		return { id: request.id, ok: true, output: { results } };
	}
	if (request.operation === "faux_sequence") {
		const {
			createFauxCore,
			fauxAssistantMessage,
			fauxText,
			fauxThinking,
			fauxToolCall,
		} = api.faux;
		const results = [];
		for (const item of request.cases) {
			const core = createFauxCore({
				api: "faux:test",
				provider: "faux",
				models: [{ id: "faux-1" }],
				tokenSize: item.tokenSize,
			});
			const blocks = (item.response.content ?? []).map((block: any) => {
				if (block.type === "text") return fauxText(block.text);
				if (block.type === "thinking") return fauxThinking(block.thinking);
				return fauxToolCall(block.name, block.arguments, { id: block.id });
			});
			const message = fauxAssistantMessage(blocks, {
				stopReason: item.response.stopReason,
				errorMessage: item.response.errorMessage,
			});
			core.setResponses([message]);
			const steps = [];
			for (const step of item.steps) {
				const stream = core.streamSimple(
					core.getModel(),
					step.context,
					step.options,
				);
				const events = await collect(stream);
				const result = await stream.result();
				steps.push({
					events: events.map((event: any) => event.type),
					result: normalizeMessage(result),
				});
			}
			results.push({ name: item.name, steps });
		}
		return { id: request.id, ok: true, output: { results } };
	}
	if (request.operation === "lazy_stream_error") {
		const { lazyStream } = api.lazy;
		const model = testModel("lazy", "m1");
		const stream = lazyStream(model, async () => {
			throw new Error("setup boom");
		});
		const events = await collect(stream);
		const result = await stream.result();
		return {
			id: request.id,
			ok: true,
			output: {
				events: events.map((event: any) => event.type),
				result: normalizeMessage(result),
			},
		};
	}
	if (request.operation === "refresh") {
		const models = createModels();
		models.setProvider(
			createProvider({
				id: "dynamic",
				auth: {
					apiKey: { name: "Ambient", resolve: async () => ({ auth: {} }) },
				},
				models: [testModel("dynamic", "static")],
				refreshModels: async () => [testModel("dynamic", "fetched")],
				api: {
					stream: () => new api.eventStream.AssistantMessageEventStream(),
					streamSimple: () => new api.eventStream.AssistantMessageEventStream(),
				},
			}),
		);
		await models.refresh("dynamic");
		return {
			id: request.id,
			ok: true,
			output: {
				models: models.getModels("dynamic").map((model: any) => model.id),
			},
		};
	}
	if (request.operation === "apply_auth") {
		const { createModels, createProvider } = api.models;
		const { AssistantMessageEventStream } = api.eventStream;
		let captured: any;
		const respond = (model: any, _context: any, options: any) => {
			captured = {
				baseUrl: model.baseUrl,
				apiKey: options?.apiKey,
				headers: options?.headers,
				env: options?.env,
			};
			const stream = new AssistantMessageEventStream();
			const message = doneMessage(model, "ok");
			stream.push({ type: "start", partial: message });
			stream.push({ type: "done", reason: "stop", message });
			stream.end(message);
			return stream;
		};
		const provider = createProvider({
			id: "p1",
			auth: {
				apiKey: {
					name: "Test",
					resolve: async () => ({
						auth: {
							apiKey: "provider-key",
							headers: {
								Authorization: "Bearer provider",
								"X-Shared": "provider",
							},
							baseUrl: "https://resolved.test/v1",
						},
						env: { PROVIDER_ONLY: "provider", SHARED: "provider" },
					}),
				},
			},
			models: [
				{
					...testModel("p1", "m1"),
					headers: { "x-model": "model", "x-shared": "model" },
				},
			],
			api: { stream: respond, streamSimple: respond },
		});
		const models = createModels();
		models.setProvider(provider);
		const model = models.getModel("p1", "m1");
		const result = await models.completeSimple(
			model,
			{ messages: [] },
			{
				apiKey: "request-key",
				headers: { "X-Request": "request", "x-shared": "request" },
				env: { REQUEST_ONLY: "request", SHARED: "request" },
			},
		);
		return {
			id: request.id,
			ok: true,
			output: {
				captured,
				stopReason: result.stopReason,
			},
		};
	}
	throw new Error(`unsupported operation: ${request.operation}`);
}

async function executeRequestLine(
	line: string,
	api: Record<string, unknown>,
): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line);
		id = request.id;
		const response = await executeAsync(request, api as Record<string, any>);
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: Record<string, unknown>): void {
	let buffer = "";
	let queue = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, api));
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("ai-runtime oracle requires a source module path");
	const models = await import(pathToFileURL(`${modulePath}/models.ts`).href);
	const faux = await import(
		pathToFileURL(`${modulePath}/providers/faux.ts`).href
	);
	const lazy = await import(pathToFileURL(`${modulePath}/api/lazy.ts`).href);
	const helpers = await import(
		pathToFileURL(`${modulePath}/auth/helpers.ts`).href
	);
	const credentialStore = await import(
		pathToFileURL(`${modulePath}/auth/credential-store.ts`).href
	);
	const resolve = await import(
		pathToFileURL(`${modulePath}/auth/resolve.ts`).href
	);
	const eventStream = await import(
		pathToFileURL(`${modulePath}/utils/event-stream.ts`).href
	);
	serveRequests({
		models,
		faux,
		lazy,
		helpers,
		credentialStore,
		resolve,
		eventStream,
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`ai-runtime oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
