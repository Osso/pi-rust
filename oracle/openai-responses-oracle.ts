import { dirname } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { format } from "node:util";
import {
	installAssistantSnapshots,
	normalizeAssistantEventClocks,
	normalizeAssistantMessageClock,
} from "./assistant-snapshots.ts";
import { pathToFileURL } from "node:url";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

let activeConsoleErrors: string[] | undefined;
const forwardConsoleError = console.error.bind(console);
console.error = (...args) => {
	if (activeConsoleErrors) activeConsoleErrors.push(format(...args));
	else forwardConsoleError(...args);
};

interface ScriptedResponse {
	status: number;
	headers?: Record<string, string>;
	chunks: string[];
}

interface Request {
	id: number;
	operation: "stream" | "streamSimple";
	captureConsole?: boolean;
	model: Record<string, unknown>;
	context: Record<string, unknown>;
	stream?: Record<string, unknown>;
	extra?: Record<string, unknown>;
	reasoning?: string;
	thinkingBudgets?: Record<string, unknown>;
	responses: ScriptedResponse[];
	abortAfterChunks?: number;
}

interface Response {
	id: number | null;
	ok: boolean;
	output?: unknown;
	error?: string;
}

type StreamFunction = (
	model: unknown,
	context: unknown,
	options?: Record<string, unknown>,
) => {
	[Symbol.asyncIterator](): AsyncIterator<Record<string, unknown>>;
	result(): Promise<unknown>;
};

type Module = {
	stream: StreamFunction;
	streamSimple: StreamFunction;
};

function normalizeError(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

const ENV_HEADERS = new Set([
	"x-stainless-os",
	"x-stainless-arch",
	"x-stainless-runtime",
	"x-stainless-runtime-version",
]);

function normalizeHeaders(
	headers: Record<string, string>,
	configuredHeaders: Set<string>,
): Record<string, string> {
	const normalized: Record<string, string> = {};
	for (const [name, value] of Object.entries(headers)) {
		normalized[name] =
			ENV_HEADERS.has(name) && !configuredHeaders.has(name) ? "<env>" : value;
	}
	return normalized;
}

function createFetch(
	scenario: Request,
	requests: Array<Record<string, unknown>>,
	controller: AbortController,
): typeof fetch {
	let responseIndex = 0;
	const configuredHeaders = new Set<string>();
	for (const headers of [scenario.model.headers, scenario.stream?.headers]) {
		if (headers !== null && typeof headers === "object") {
			for (const name of Object.keys(headers))
				configuredHeaders.add(name.toLowerCase());
		}
	}
	return (async (input: string | URL | Request, init?: RequestInit) => {
		const request = init ?? {};
		const body =
			typeof request.body === "string"
				? JSON.parse(request.body)
				: request.body;
		const headerEntries = Object.fromEntries(
			new Headers(request.headers).entries(),
		);
		requests.push({
			url: String(input),
			method: request.method ?? "GET",
			headers: normalizeHeaders(headerEntries, configuredHeaders),
			body,
		});

		const scripted =
			scenario.responses[
				Math.min(responseIndex, scenario.responses.length - 1)
			]!;
		responseIndex++;

		let chunkIndex = 0;
		const stream = new ReadableStream<Uint8Array>({
			pull(streamController) {
				if (chunkIndex >= scripted.chunks.length) {
					streamController.close();
					return;
				}
				const chunk = scripted.chunks[chunkIndex++]!;
				streamController.enqueue(new TextEncoder().encode(chunk));
				if (
					scenario.abortAfterChunks !== undefined &&
					chunkIndex === scenario.abortAfterChunks
				) {
					controller.abort();
					streamController.error(
						new DOMException("This operation was aborted", "AbortError"),
					);
				}
			},
		});

		return new Response(stream, {
			status: scripted.status,
			headers: {
				"content-type": "text/event-stream",
				...(scripted.headers ?? {}),
			},
		});
	}) as typeof fetch;
}

async function execute(request: Request, module: Module): Promise<unknown> {
	const controller = new AbortController();
	const requests: Array<Record<string, unknown>> = [];
	setFetchScript(createFetch(request, requests, controller));
	const consoleErrors: string[] = [];
	const previousCapture = activeConsoleErrors;
	if (request.captureConsole) activeConsoleErrors = consoleErrors;
	try {
		const baseOptions: Record<string, unknown> = {
			...(request.stream ?? {}),
			...(request.extra ?? {}),
			apiKey: (request.stream?.apiKey as string | undefined) ?? "test-key",
			...(request.abortAfterChunks !== undefined
				? { signal: controller.signal }
				: {}),
		};
		const options: Record<string, unknown> =
			request.operation === "streamSimple"
				? {
						...baseOptions,
						...(request.reasoning !== undefined
							? { reasoning: request.reasoning }
							: {}),
						...(request.thinkingBudgets !== undefined
							? { thinkingBudgets: request.thinkingBudgets }
							: {}),
					}
				: baseOptions;
		const events: Array<unknown> = [];
		const stream = module[request.operation](
			request.model,
			request.context,
			options,
		) as unknown as {
			push: (event: unknown) => void;
			[Symbol.asyncIterator](): AsyncIterator<Record<string, unknown>>;
			result(): Promise<unknown>;
		};
		const originalPush = stream.push.bind(stream);
		stream.push = (event: unknown) => {
			events.push(normalizeAssistantEventClocks(event));
			originalPush(event);
		};
		for await (const _event of stream) {
			// Events are snapshotted at push time above.
		}
		let result: unknown;
		try {
			result = normalizeAssistantMessageClock(await stream.result());
		} catch (error) {
			result = { error: normalizeError(error) };
		}
		const output = { requests, events, result };
		return request.captureConsole ? { ...output, consoleErrors } : output;
	} finally {
		activeConsoleErrors = previousCapture;
		clearFetchScript();
	}
}

function executeRequestLine(line: string, module: Module): Promise<void> {
	let id: number | null = null;
	let request: Request;
	try {
		request = JSON.parse(line) as Request;
		id = request.id;
	} catch (error) {
		process.stdout.write(
			`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
		);
		return Promise.resolve();
	}
	return execute(request, module)
		.then((output) => {
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		})
		.catch((error: unknown) => {
			process.stdout.write(
				`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
			);
		});
}

function serveRequests(module: Module): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, module));
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("openai responses oracle requires a source module path");
	installScriptedFetch();
	await installAssistantSnapshots(dirname(dirname(modulePath)));
	const module = (await import(pathToFileURL(modulePath).href)) as Module;
	serveRequests(module);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`openai responses oracle startup failed: ${normalizeError(error)}\n`,
	);
	process.exitCode = 1;
});
