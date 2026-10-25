import { dirname, join } from "node:path";
import {
	normalizeAssistantEventClocks as normalizeEvent,
	normalizeAssistantMessageClock as normalize,
} from "./assistant-snapshots.ts";
import { pathToFileURL } from "node:url";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

type ScenarioOptions = {
	apiKey?: string;
	temperature?: number;
	maxTokens?: number;
	promptMode?: "reasoning";
	reasoningEffort?: "none" | "high";
	toolChoice?: unknown;
	sessionId?: string;
	cacheRetention?: "none" | "short" | "long";
	headers?: Record<string, string | null>;
	onPayload?: "augment";
	observePayload?: boolean;
};

type Scenario = {
	id: number;
	api?: "stream" | "streamSimple";
	model: Record<string, unknown>;
	context: Record<string, unknown>;
	options: ScenarioOptions;
	response: {
		status?: number;
		statusText?: string;
		headers?: Record<string, string>;
		chunks?: string[];
		chunking?: "chunks" | "bytes";
	};
	abort?: "before-read";
};

type CapturedRequest = {
	url: string;
	method: string;
	headers: Record<string, string>;
	body: unknown;
};

type StreamApi = {
	stream(
		model: unknown,
		context: unknown,
		options: Record<string, unknown>,
	): AsyncIterable<unknown> & { result(): Promise<unknown> };
	streamSimple(
		model: unknown,
		context: unknown,
		options: Record<string, unknown>,
	): AsyncIterable<unknown> & { result(): Promise<unknown> };
};

// Streaming scratch state (`partialArgs`) never reaches a persisted message and
// `Date.now()`/`Error.stack` are environment-derived; both are normalized here.
// TypeScript pushes event objects whose `partial` is a live reference to the
// streaming message. Snapshotting on `push` (instead of when the consumer
// receives the event) reproduces the module's emitted event sequence; Rust
// clones the partial at push time.
let currentCapture: unknown[] | undefined;

type EventStreamClass = {
	prototype: { push(event: unknown): void };
};

async function patchEventStream(modulePath: string): Promise<void> {
	const eventStreamPath = join(
		dirname(modulePath),
		"..",
		"utils",
		"event-stream.ts",
	);
	const module = (await import(pathToFileURL(eventStreamPath).href)) as {
		AssistantMessageEventStream: EventStreamClass;
	};
	const prototype = module.AssistantMessageEventStream.prototype;
	const originalPush = prototype.push;
	prototype.push = function (this: unknown, event: unknown) {
		if (currentCapture) {
			currentCapture.push(normalizeEvent(JSON.parse(JSON.stringify(event))));
		}
		originalPush.call(this, event);
	};
}

function augmentPayload(
	payload: Record<string, unknown>,
): Record<string, unknown> {
	return {
		...payload,
		topP: 0.9,
		randomSeed: 42,
		responseFormat: {
			type: "json_schema",
			jsonSchema: {
				name: "result",
				schemaDefinition: {
					type: "object",
					properties: { maxTokens: { type: "number" } },
				},
			},
		},
		presencePenalty: 0.1,
		frequencyPenalty: 0.2,
		parallelToolCalls: true,
		safePrompt: true,
	};
}

function createResponse(
	scenario: Scenario,
	captured: { abort?: () => void },
): Response {
	const response = scenario.response;
	const chunks = response.chunks ?? [];
	const body = new ReadableStream<Uint8Array>({
		start(controller) {
			const encoder = new TextEncoder();
			if (response.chunking === "bytes") {
				for (const chunk of chunks) {
					for (const byte of encoder.encode(chunk))
						controller.enqueue(Uint8Array.of(byte));
				}
			} else {
				for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
			}
			if (scenario.abort === "before-read") {
				captured.abort?.();
				controller.close();
				return;
			}
			controller.close();
		},
	});
	return new Response(body, {
		status: response.status ?? 200,
		statusText: response.statusText ?? "",
		headers: {
			"content-type": "text/event-stream",
			...(response.headers ?? {}),
		},
	});
}

async function execute(scenario: Scenario, api: StreamApi): Promise<unknown> {
	const captured: CapturedRequest[] = [];
	let onResponse: unknown;
	const abort = { abort: undefined as (() => void) | undefined };
	const controller = new AbortController();
	abort.abort = () => controller.abort();

	const fetch = async (
		input: string | URL | Request,
		init?: RequestInit,
	): Promise<Response> => {
		const request = new Request(input, init);
		captured.push({
			url: request.url,
			method: request.method,
			headers: Object.fromEntries(request.headers),
			body: request.body === null ? undefined : await request.clone().text(),
		});
		const response = createResponse(scenario, abort);
		if (request.signal.aborted) throw request.signal.reason;
		return response;
	};

	setFetchScript(fetch);
	const options: Record<string, unknown> = { ...scenario.options };
	if (scenario.options.onPayload === "augment") {
		options.onPayload = (payload: Record<string, unknown>) =>
			augmentPayload(payload);
	}
	let onPayload: string | undefined;
	if (scenario.options.observePayload) {
		options.onPayload = (payload: Record<string, unknown>) => {
			onPayload = JSON.stringify(payload);
			return scenario.options.onPayload === "augment" ? augmentPayload(payload) : undefined;
		};
	}
	options.onResponse = (response: {
		status: number;
		headers: Record<string, string>;
	}) => {
		onResponse = { status: response.status, headers: response.headers };
	};
	if (scenario.abort) options.signal = controller.signal;

	const events: unknown[] = [];
	currentCapture = events;
	try {
		const run = scenario.api === "streamSimple" ? api.streamSimple : api.stream;
		const stream = run(scenario.model, scenario.context, options);
		for await (const _event of stream) {
			// Events are captured at push time by patchEventStream.
		}
		const message = await stream.result();
		currentCapture = undefined;
		return {
			id: scenario.id,
			ok: true,
			request: captured[0] ?? null,
			onResponse: onResponse ?? null,
			...(scenario.options.observePayload ? { onPayload: onPayload ?? null } : {}),
			events: events.map(normalizeEvent),
			message: normalize(message),
		};
	} catch (error) {
		currentCapture = undefined;
		return {
			id: scenario.id,
			ok: false,
			request: captured[0] ?? null,
			onResponse: onResponse ?? null,
			events,
			error:
				error instanceof Error
					? { name: error.name, message: error.message }
					: { message: String(error) },
		};
	} finally {
		clearFetchScript();
	}
}

function serve(api: StreamApi): void {
	let buffer = "";
	process.stdin.setEncoding("utf8");
	process.stdin.on("data", (chunk: string) => {
		buffer += chunk;
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			void execute(JSON.parse(line) as Scenario, api).then((response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("mistral oracle requires a source module path");
	installScriptedFetch();
	const api = (await import(pathToFileURL(modulePath).href)) as StreamApi;
	await patchEventStream(modulePath);
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`mistral oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
