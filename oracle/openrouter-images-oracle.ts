import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

interface FixtureResponse {
	status: number;
	headers?: Record<string, string>;
	body: string;
	bodyDelayMs?: number;
}

interface Request {
	id: number;
	model: Record<string, unknown>;
	context: { input: Array<Record<string, unknown>> };
	options: {
		apiKey?: string;
		headers?: Record<string, string>;
		timeoutMs?: number;
		maxRetries?: number;
		aborted?: boolean;
		response?: FixtureResponse;
		replacePayload?: unknown;
		payloadError?: unknown;
		responseError?: unknown;
	};
}

interface RecordedRequest {
	method: string;
	url: string;
	headers: Record<string, string>;
	body: string | null;
}

type ImagesApi = {
	generateImages(
		model: unknown,
		context: unknown,
		options: Record<string, unknown>,
	): Promise<Record<string, unknown>>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function normalizeHeaders(headers: unknown): Record<string, string> {
	const record =
		headers instanceof Headers
			? Object.fromEntries(headers.entries())
			: Array.isArray(headers)
				? Object.fromEntries(headers as Array<[string, string]>)
				: headers && typeof headers === "object"
					? Object.fromEntries(
							Object.entries(headers).map(([name, value]) => [
								name,
								String(value),
							]),
						)
					: {};
	return Object.fromEntries(
		Object.entries(record).filter(
			([name]) => name !== "user-agent" && (!name.startsWith("x-stainless-") || name === "x-stainless-retry-count"),
		),
	);
}

async function execute(
	request: Request,
	api: ImagesApi,
): Promise<Record<string, unknown>> {
	const recorded: RecordedRequest = {
		method: "",
		url: "",
		headers: {},
		body: null,
	};
	const response = request.options.response ?? {
		status: 200,
		body: JSON.stringify({ id: "img-1", choices: [] }),
	};
	const controller = new AbortController();
	if (request.options.aborted) controller.abort();

	const fetchImpl: typeof globalThis.fetch = async (url, init) => {
		recorded.method = init?.method ?? "GET";
		recorded.url = String(url);
		recorded.headers = normalizeHeaders(init?.headers);
		recorded.body = typeof init?.body === "string" ? init.body : null;
		if (init?.signal?.aborted) throw new Error("This operation was aborted");
		const body = response.bodyDelayMs === undefined ? response.body : new ReadableStream({
			start(controller) {
				setTimeout(() => {
					controller.enqueue(new TextEncoder().encode(response.body));
					controller.close();
				}, response.bodyDelayMs);
			},
		});
		return new Response(body, {
			status: response.status,
			headers: response.headers ?? {},
		});
	};

	setFetchScript(fetchImpl);
	try {
		const output = await api.generateImages(request.model, request.context, {
			apiKey: request.options.apiKey,
			maxRetries: request.options.maxRetries,
			headers: request.options.headers,
			onPayload: () => {
				if (Object.hasOwn(request.options, "payloadError")) throw request.options.payloadError;
				return request.options.replacePayload;
			},
			onResponse: () => {
				if (Object.hasOwn(request.options, "responseError")) throw request.options.responseError;
			},
			...(request.options.timeoutMs === undefined
				? {}
				: { timeoutMs: request.options.timeoutMs }),
			...(request.options.aborted ? { signal: controller.signal } : {}),
		});
		return {
			id: request.id,
			ok: true,
			request: recorded,
			output: { ...output, timestamp: 0 },
		};
	} catch (error) {
		return {
			id: request.id,
			ok: false,
			request: recorded,
			error: normalizeError(error),
		};
	} finally {
		clearFetchScript();
	}
}

function serve(api: ImagesApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const request = JSON.parse(buffer.slice(0, lineFeed)) as Request;
			buffer = buffer.slice(lineFeed + 1);
			void execute(request, api).then((response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("openrouter images oracle requires a source module path");
	installScriptedFetch();
	const api = (await import(pathToFileURL(modulePath).href)) as ImagesApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`openrouter images oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
