import { dirname } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Model = Record<string, unknown>;

type StreamOptions = {
	apiKey?: string;
	env?: Record<string, string>;
	reasoning?: string;
};

type CompatApi = {
	registerApiProvider(provider: unknown, sourceId?: string): void;
	getApiProvider(api: string): { api: string; stream: (m: Model, c: unknown, o?: unknown) => any } | undefined;
	getApiProviders(): Array<{ api: string }>;
	unregisterApiProviders(sourceId: string): void;
	resetApiProviders(): void;
	stream(model: Model, context: unknown, options?: unknown): any;
	streamSimple(model: Model, context: unknown, options?: unknown): any;
	complete(model: Model, context: unknown, options?: unknown): Promise<any>;
	registerFauxProvider(options?: unknown): any;
	getModel(provider: string, modelId: string): Model | undefined;
	getModels(provider: string): Model[];
	getProviders(): string[];
};

type Request =
	| { id: RequestId; op: "registry" }
	| {
			id: RequestId;
			op: "dispatch";
			cases: Array<{
				name: string;
				api: string;
				provider: string;
				modelId: string;
				options?: StreamOptions;
				simple?: boolean;
			}>;
	  }
	| { id: RequestId; op: "mismatch" }
	| { id: RequestId; op: "missing" }
	| { id: RequestId; op: "faux" };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function model(api: string, provider: string, id: string): Model {
	return {
		id,
		name: "Test Model",
		api,
		provider,
		baseUrl: "https://example.test/v1",
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 128000,
		maxTokens: 4096,
	};
}

const context = { messages: [{ role: "user", content: "hi there", timestamp: 0 }] };

function createFakeProvider(api: string, EventStream: any, record: (options: any) => void) {
	const run = (options: any) => {
		record(options ?? {});
		const stream = new EventStream();
		const output = {
			role: "assistant",
			content: [{ type: "text", text: "ok" }],
			api,
			provider: "custom-openai",
			model: "test-model",
			usage: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
				totalTokens: 0,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
			stopReason: "stop",
			timestamp: 0,
		};
		stream.push({ type: "start", partial: output });
		stream.push({ type: "done", reason: "stop", message: output });
		stream.end(output);
		return stream;
	};
	return {
		api,
		stream: (_model: Model, _context: unknown, options?: unknown) => run(options),
		streamSimple: (_model: Model, _context: unknown, options?: unknown) => run(options),
	};
}

async function executeRegistry(compat: CompatApi) {
	compat.resetApiProviders();
	const initial = compat.getApiProviders().map((provider) => provider.api);
	compat.registerApiProvider(createFakeProvider("openai-responses", null, () => {}), "source-a");
	compat.registerApiProvider(createFakeProvider("openai-completions", null, () => {}), "source-b");
	const afterRegister = compat.getApiProviders().map((provider) => provider.api);
	compat.unregisterApiProviders("source-a");
	const afterUnregister = compat.getApiProviders().map((provider) => provider.api);
	compat.resetApiProviders();
	const afterReset = compat.getApiProviders().map((provider) => provider.api);
	return { initial, afterRegister, afterUnregister, afterReset };
}

async function executeDispatch(
	compat: CompatApi,
	EventStream: any,
	request: Extract<Request, { op: "dispatch" }>,
) {
	const results = [];
	for (const testCase of request.cases) {
		compat.resetApiProviders();
		const seen: Array<{ apiKey: string | null; reasoning: string | null }> = [];
		compat.registerApiProvider(
			createFakeProvider(testCase.api, EventStream, (options) => {
				seen.push({
					apiKey: options.apiKey ?? null,
					reasoning: options.reasoning ?? null,
				});
			}),
		);
		const target = model(testCase.api, testCase.provider, testCase.modelId);
		const options = testCase.options ?? {};
		const result = testCase.simple
			? await compat.streamSimple(target, context, options).result()
			: await compat.complete(target, context, options);
		results.push({
			name: testCase.name,
			seen,
			content: result.content,
			stopReason: result.stopReason ?? null,
			errorMessage: result.errorMessage ?? null,
		});
	}
	compat.resetApiProviders();
	return { results };
}

async function executeMismatch(compat: CompatApi) {
	compat.resetApiProviders();
	compat.registerApiProvider(createFakeProvider("openai-responses", null, () => {}));
	const provider = compat.getApiProvider("openai-responses");
	try {
		provider?.stream(model("other-api", "custom", "m"), context, {});
		return { error: null };
	} catch (error) {
		return { error: normalizeError(error) };
	} finally {
		compat.resetApiProviders();
	}
}

async function executeMissing(compat: CompatApi) {
	compat.resetApiProviders();
	try {
		compat.stream(model("missing-api", "custom", "m"), context, {});
		return { error: null };
	} catch (error) {
		return { error: normalizeError(error) };
	}
}

async function executeFaux(compat: CompatApi, srcDir: string) {
	const fauxModule = (await import(pathToFileURL(`${srcDir}/providers/faux.ts`).href)) as {
		fauxAssistantMessage(text: string): unknown;
	};
	compat.resetApiProviders();
	const registration = compat.registerFauxProvider({ tokenSize: { min: 3, max: 5 } });
	registration.setResponses([fauxModule.fauxAssistantMessage("hello world")]);
	const target = registration.getModel();
	const result = await compat.complete(target, context);
	const output = {
		apiPrefix: String(registration.api).startsWith("faux:"),
		content: result.content,
		inputPositive: result.usage.input > 0,
		outputPositive: result.usage.output > 0,
		totalMatches: result.usage.totalTokens === result.usage.input + result.usage.output,
		callCount: registration.state.callCount,
		pending: registration.getPendingResponseCount(),
	};
	registration.unregister();
	const afterUnregister = compat.getApiProviders().map((provider) => provider.api);
	compat.resetApiProviders();
	return { output, afterUnregister };
}

async function execute(request: Request, compat: CompatApi, srcDir: string) {
	switch (request.op) {
		case "registry":
			return executeRegistry(compat);
		case "dispatch":
			return executeDispatch(compat, await loadEventStream(srcDir), request);
		case "mismatch":
			return executeMismatch(compat);
		case "missing":
			return executeMissing(compat);
		case "faux":
			return executeFaux(compat, srcDir);
	}
}

let eventStreamClass: any = null;

async function loadEventStream(srcDir: string) {
	if (eventStreamClass === null) {
		const module = (await import(pathToFileURL(`${srcDir}/utils/event-stream.ts`).href)) as {
			AssistantMessageEventStream: unknown;
		};
		eventStreamClass = module.AssistantMessageEventStream;
	}
	return eventStreamClass;
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("missing module path");
	const srcDir = dirname(modulePath);
	const compat = (await import(pathToFileURL(modulePath).href)) as CompatApi;
	const readline = await import("node:readline");
	const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of lines) {
		if (line.trim().length === 0) continue;
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const output = await execute(request, compat, srcDir);
			console.log(JSON.stringify({ id, ok: true, output }));
		} catch (error) {
			console.log(JSON.stringify({ id, ok: false, error: normalizeError(error) }));
		}
	}
}

main().catch((error: unknown) => {
	console.error(error);
	process.exit(1);
});
