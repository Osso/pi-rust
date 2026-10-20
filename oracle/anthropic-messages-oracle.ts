import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = number;

type ScriptedResponse = {
	status: number;
	headers: Array<[string, string]>;
	chunks: string[];
	abortAtChunk?: number;
	fetchError?: string;
};

type Request = {
	id: RequestId;
	kind: "stream" | "streamSimple";
	model: unknown;
	context: unknown;
	options: Record<string, unknown>;
	script: ScriptedResponse[];
};

type RecordedRequest = {
	method: string;
	url: string;
	headers: Record<string, string>;
	body: string | null;
};

type AnthropicApi = {
	stream(model: unknown, context: unknown, options?: Record<string, unknown>): AsyncIterable<unknown> & {
		result(): Promise<unknown>;
	};
	streamSimple(model: unknown, context: unknown, options?: Record<string, unknown>): AsyncIterable<unknown> & {
		result(): Promise<unknown>;
	};
};

type Response =
	| { id: RequestId; ok: true; output: { requests: RecordedRequest[]; events: unknown[]; result: unknown } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function normalizeMessage(message: unknown): unknown {
	if (!message || typeof message !== "object") return message;
	const copy: Record<string, unknown> = { ...(message as Record<string, unknown>), timestamp: 0 };
	return copy;
}

function normalizeEvent(event: unknown): unknown {
	if (!event || typeof event !== "object") return event;
	// Events carry references to the live `output` message; clone so each recorded
	// event is the snapshot that existed when the event was pushed.
	const copy = structuredClone(event) as Record<string, unknown>;
	for (const key of ["partial", "message", "error"]) {
		if (copy[key]) copy[key] = normalizeMessage(copy[key]);
	}
	return copy;
}

function getUrl(input: unknown): string {
	if (typeof input === "string") return input;
	if (input instanceof URL) return input.toString();
	if (input instanceof Request) return input.url;
	throw new Error(`Unsupported fetch input: ${String(input)}`);
}

function createScriptedFetch(script: ScriptedResponse[], controller: AbortController, recorded: RecordedRequest[]) {
	return async (input: unknown, init?: RequestInit): Promise<Response> => {
		const headers = new Headers(init?.headers);
		const body = typeof init?.body === "string" ? init.body : init?.body == null ? null : String(init.body);
		recorded.push({
			method: (init?.method ?? "GET").toUpperCase(),
			url: getUrl(input),
			headers: Object.fromEntries(headers.entries()),
			body,
		});

		const step = script.shift();
		if (!step) throw new Error("no scripted response");
		if (step.fetchError !== undefined) throw new Error(step.fetchError);

		const chunks = step.chunks;
		let index = 0;
		const stream = new ReadableStream<Uint8Array>({
			pull(streamController) {
				if (index >= chunks.length) {
					streamController.close();
					return;
				}
				if (step.abortAtChunk === index) {
					controller.abort();
					streamController.error(new Error("This operation was aborted"));
					return;
				}
				streamController.enqueue(new TextEncoder().encode(chunks[index]));
				index += 1;
			},
		});
		return new globalThis.Response(stream, { status: step.status, headers: step.headers });
	};
}

async function execute(request: Request, api: AnthropicApi): Promise<Response> {
	const controller = new AbortController();
	const recorded: RecordedRequest[] = [];
	const options: Record<string, unknown> = {
		...request.options,
		fetch: createScriptedFetch(request.script, controller, recorded),
		signal: controller.signal,
	};

	try {
		const stream =
			request.kind === "streamSimple"
				? api.streamSimple(request.model, request.context, options)
				: api.stream(request.model, request.context, options);
		const events: unknown[] = [];
		for await (const event of stream) {
			events.push(normalizeEvent(event));
		}
		const result = normalizeMessage(await stream.result());
		return { id: request.id, ok: true, output: { requests: recorded, events, result } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: AnthropicApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			let id: RequestId = 0;
			void (async () => {
				try {
					const request = JSON.parse(line) as Request;
					id = request.id;
					process.stdout.write(`${JSON.stringify(await execute(request, api))}\n`);
				} catch (error) {
					process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
				}
			})();
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("anthropic messages oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as AnthropicApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`anthropic messages oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
