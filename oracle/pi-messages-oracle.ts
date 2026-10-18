import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

type Scenario = {
	id: number;
	api?: "stream" | "streamSimple";
	model: Record<string, unknown>;
	context: Record<string, unknown>;
	options: {
		apiKey?: string;
		temperature?: number;
		maxTokens?: number;
		reasoning?: string;
		toolChoice?: unknown;
		debug?: boolean;
		sessionId?: string;
		cacheRetention?: "none" | "short" | "long";
		headers?: Record<string, string | null>;
		env?: Record<string, string>;
		onPayload?: "replace";
	};
	response?: {
		status?: number;
		statusText?: string;
		headers?: Record<string, string>;
		chunks?: string[];
		chunking?: "chunks" | "bytes";
	};
	fetchError?: "abort" | string;
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

// `partialArgs` is JS streaming scratch; timestamps and stacks are
// environment-derived. `contentSignature`/`redacted` are extra fields the
// converter spreads into end events but that are not part of the
// `AssistantMessageEvent` contract. Property order is preserved so the
// differential compares the exact serialized shape.
function normalize(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(normalize);
	if (value === null || typeof value !== "object") return value;
	const source = value as Record<string, unknown>;
	const result: Record<string, unknown> = {};
	for (const key of Object.keys(source)) {
		if (key === "partialArgs" || key === "stack") continue;
		if (key === "timestamp" || key === "timestampMs") {
			result[key] = 0;
			continue;
		}
		if (key === "errorMessage" && typeof source[key] === "string" && /JSON|Unexpected token/.test(source[key] as string)) {
			result[key] = "<json-parse-error>";
			continue;
		}
		if (
			(key === "contentSignature" || key === "redacted") &&
			(source.type === "text_end" || source.type === "thinking_end") &&
			"partial" in source
		) {
			continue;
		}
		if ((key === "id" || key === "toolName") && source.type === "toolcall_start" && "partial" in source) {
			continue;
		}
		result[key] = normalize(source[key]);
	}
	return result;
}

let currentCapture: unknown[] | undefined;

type EventStreamClass = {
	prototype: { push(event: unknown): void };
};

async function patchEventStream(modulePath: string): Promise<void> {
	const eventStreamPath = join(dirname(modulePath), "..", "utils", "event-stream.ts");
	const module = (await import(pathToFileURL(eventStreamPath).href)) as {
		AssistantMessageEventStream: EventStreamClass;
	};
	const prototype = module.AssistantMessageEventStream.prototype;
	const originalPush = prototype.push;
	prototype.push = function (this: unknown, event: unknown) {
		if (currentCapture) currentCapture.push(normalize(JSON.parse(JSON.stringify(event))));
		originalPush.call(this, event);
	};
}

function createResponse(scenario: Scenario): Response {
	const response = scenario.response ?? {};
	const chunks = response.chunks ?? [];
	const body = new ReadableStream<Uint8Array>({
		start(controller) {
			const encoder = new TextEncoder();
			if (response.chunking === "bytes") {
				for (const chunk of chunks) {
					for (const byte of encoder.encode(chunk)) controller.enqueue(Uint8Array.of(byte));
				}
			} else {
				for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
			}
			controller.close();
		},
	});
	return new Response(body, {
		status: response.status ?? 200,
		statusText: response.statusText ?? "",
		headers: { "content-type": "text/event-stream", ...(response.headers ?? {}) },
	});
}

async function execute(scenario: Scenario, api: StreamApi): Promise<unknown> {
	const captured: unknown[] = [];
	let onResponse: unknown;
	const controller = new AbortController();

	const fetch = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
		captured.push({
			url: String(input),
			method: init?.method ?? "GET",
			headers: Object.fromEntries(new Headers(init?.headers)),
			body: init?.body === undefined ? undefined : String(init.body),
		});
		if (scenario.fetchError === "abort") {
			controller.abort();
			throw new Error("This operation was aborted");
		}
		if (scenario.fetchError !== undefined) throw new Error(scenario.fetchError);
		return createResponse(scenario);
	};

	const options: Record<string, unknown> = { ...scenario.options, fetch };
	if (scenario.options.onPayload === "replace") {
		options.onPayload = (payload: Record<string, unknown>) => ({ ...payload, marker: true });
	}
	options.onResponse = (response: { status: number; headers: Record<string, string> }) => {
		onResponse = { status: response.status, headers: response.headers };
	};
	if (scenario.fetchError === "abort") options.signal = controller.signal;

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
			events: events.map(normalize),
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
			error: error instanceof Error ? { name: error.name, message: error.message } : { message: String(error) },
		};
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
	if (modulePath === undefined) throw new Error("pi-messages oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as StreamApi;
	await patchEventStream(modulePath);
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`pi-messages oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
