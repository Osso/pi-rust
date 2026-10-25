import * as fs from "node:fs";
import { installAssistantSnapshots, normalizeAssistantMessageClock } from "./assistant-snapshots.ts";
import * as net from "node:net";
import * as path from "node:path";
import { pathToFileURL } from "node:url";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";
import * as tls from "node:tls";

type RequestId = string | number | null;

type ChunkSpec = string | { base64: string };

type ScriptedResponse = {
	status?: number;
	statusText?: string;
	headers?: Record<string, string>;
	chunks?: ChunkSpec[];
	throw?: { name?: string; message: string };
};

type RunRequest = {
	id: RequestId;
	api: "google-generative-ai" | "google-vertex";
	operation: "stream" | "streamSimple";
	model: Record<string, unknown>;
	context: Record<string, unknown>;
	stream?: Record<string, unknown>;
	extra?: Record<string, unknown>;
	reasoning?: string;
	thinkingBudgets?: Record<string, unknown>;
	responses: ScriptedResponse[];
	tokenResponses?: ScriptedResponse[];
	abortAfterChunks?: number;
	preAborted?: boolean;
	onPayloadPatch?: unknown;
};

type ApiModule = {
	stream: StreamFunction;
	streamSimple: StreamFunction;
};

type StreamFunction = (
	model: unknown,
	context: unknown,
	options?: Record<string, unknown>,
) => AsyncIterable<Record<string, unknown>> & {
	push?: (event: unknown) => void;
	result(): Promise<unknown>;
};

type RecordedRequest = {
	method: string;
	url: string;
	headers: Record<string, string>;
	body: string | null;
	jwt?: { header: unknown; claims: unknown; signatureLength: number } | null;
};

function normalizeEvent(value: unknown): unknown {
	const copy = structuredClone(value) as Record<string, unknown>;
	for (const key of ["partial", "message", "error"]) {
		if (copy[key] !== undefined) copy[key] = normalizeAssistantMessageClock(copy[key]);
	}
	return copy;
}

function decodeBase64Url(text: string): string {
	return Buffer.from(text, "base64url").toString("utf8");
}

function normalizeBody(body: string | null): {
	body: string | null;
	jwt: RecordedRequest["jwt"];
} {
	if (body === null || !body.includes("assertion=")) return { body, jwt: null };
	const assertion = new URLSearchParams(body).get("assertion");
	if (!assertion) return { body, jwt: null };
	const [header, claims, signature] = assertion.split(".");
	const normalized = {
		header: JSON.parse(decodeBase64Url(header ?? "")),
		claims: JSON.parse(decodeBase64Url(claims ?? "")),
		signatureLength: (signature ?? "").length,
	} as {
		header: Record<string, unknown>;
		claims: Record<string, unknown>;
		signatureLength: number;
	};
	normalized.claims.iat = 0;
	normalized.claims.exp = 0;
	return { body: body.replace(assertion, "<jwt>"), jwt: normalized };
}

function normalizeSdkHeaders(headers: Headers): Record<string, string> {
	return Object.fromEntries(headers);
}

function encodeChunk(chunk: ChunkSpec): Uint8Array {
	if (typeof chunk === "string") return new TextEncoder().encode(chunk);
	return new Uint8Array(Buffer.from(chunk.base64, "base64"));
}

let activeRequests: RecordedRequest[] = [];
let activeTokenResponses: ScriptedResponse[] = [];

const PROXY_FIXTURES = path.join(__dirname, "fixtures");

function startTokenProxy(): Promise<number> {
	const secureContext = tls.createSecureContext({
		cert: fs.readFileSync(path.join(PROXY_FIXTURES, "oauth-proxy-cert.pem")),
		key: fs.readFileSync(path.join(PROXY_FIXTURES, "oauth-proxy-key.pem")),
	});
	const server = net.createServer((socket) => {
		let handshake = Buffer.alloc(0);
		const onHandshake = (chunk: Buffer) => {
			handshake = Buffer.concat([handshake, chunk]);
			const headerEnd = handshake.indexOf("\r\n\r\n");
			if (headerEnd === -1) return;
			socket.off("data", onHandshake);
			const requestLine =
				handshake.subarray(0, headerEnd).toString("utf8").split("\r\n")[0] ??
				"";
			const target = requestLine.split(" ")[1] ?? "oauth2.googleapis.com:443";
			socket.write("HTTP/1.1 200 Connection Established\r\n\r\n");
			const tlsSocket = new tls.TLSSocket(socket, {
				isServer: true,
				secureContext,
			});
			serveTokenRequest(
				tlsSocket,
				target.split(":")[0] ?? "oauth2.googleapis.com",
			);
		};
		socket.on("data", onHandshake);
		socket.on("error", () => {});
	});
	server.unref();
	return new Promise((resolve) => {
		server.listen(0, "127.0.0.1", () => {
			resolve((server.address() as net.AddressInfo).port);
		});
	});
}

function serveTokenRequest(socket: tls.TLSSocket, host: string): void {
	let buffer = Buffer.alloc(0);
	const onData = (chunk: Buffer) => {
		buffer = Buffer.concat([buffer, chunk]);
		const headerEnd = buffer.indexOf("\r\n\r\n");
		if (headerEnd === -1) return;
		const lines = buffer.subarray(0, headerEnd).toString("utf8").split("\r\n");
		const [method, requestPath] = (lines[0] ?? "").split(" ");
		const headers: Record<string, string> = {};
		for (const line of lines.slice(1)) {
			const separator = line.indexOf(":");
			if (separator > 0)
				headers[line.slice(0, separator).toLowerCase()] = line
					.slice(separator + 1)
					.trim();
		}
		const length = Number(headers["content-length"] ?? "0");
		const bodyStart = headerEnd + 4;
		if (buffer.length < bodyStart + length) return;
		socket.off("data", onData);

		const body = buffer
			.subarray(bodyStart, bodyStart + length)
			.toString("utf8");
		const normalized = normalizeBody(body.length > 0 ? body : null);
		const recorded: RecordedRequest = {
			method: method ?? "POST",
			url: `https://${host}${requestPath ?? ""}`,
			headers: {
				accept: headers["accept"] ?? "",
				"content-type": headers["content-type"] ?? "",
			},
			body: normalized.body,
		};
		if (normalized.jwt) recorded.jwt = normalized.jwt;
		activeRequests.push(recorded);

		const scripted = activeTokenResponses.shift();
		const status = scripted?.status ?? 200;
		const payload = (scripted?.chunks ?? [])
			.map((chunk) =>
				typeof chunk === "string"
					? chunk
					: Buffer.from(chunk.base64, "base64").toString("utf8"),
			)
			.join("");
		const responseHeaders = {
			"content-type": "application/json",
			...(scripted?.headers ?? {}),
		};
		const head = [
			`HTTP/1.1 ${status} ${scripted?.statusText ?? "OK"}`,
			...Object.entries(responseHeaders).map(
				([name, value]) => `${name}: ${value}`,
			),
			`content-length: ${Buffer.byteLength(payload)}`,
			"connection: close",
			"",
			"",
		].join("\r\n");
		socket.write(head + payload);
		socket.end();
	};
	socket.on("data", onData);
	socket.on("error", () => {});
}

function installFetch(
	responses: ScriptedResponse[],
	requests: RecordedRequest[],
	controller: AbortController,
	abortAfterChunks: number | undefined,
): typeof fetch {
	let responseIndex = 0;
	return (async (input: unknown, init: Record<string, unknown> = {}) => {
		const url =
			typeof input === "string"
				? input
				: String((input as { url: string }).url);
		const method = (init.method as string | undefined) ?? "GET";
		const headers = new Headers(init.headers as HeadersInit | undefined);
		const rawBody = init.body as string | URLSearchParams | undefined;
		let body: string | null = null;
		if (rawBody instanceof URLSearchParams) {
			body = rawBody.toString();
			if (!headers.has("content-type")) {
				headers.set(
					"content-type",
					"application/x-www-form-urlencoded;charset=UTF-8",
				);
			}
		} else if (typeof rawBody === "string") {
			body = rawBody;
		}
		const normalized = normalizeBody(body);
		const request: RecordedRequest = {
			method,
			url,
			headers: normalizeSdkHeaders(headers),
			body: normalized.body,
		};
		if (normalized.jwt) request.jwt = normalized.jwt;
		requests.push(request);

		const scripted = responses[Math.min(responseIndex, responses.length - 1)];
		responseIndex++;
		if (!scripted) throw new Error("unexpected fetch call");
		if (scripted.throw) {
			const error = new Error(scripted.throw.message);
			if (scripted.throw.name) error.name = scripted.throw.name;
			throw error;
		}

		const chunks = scripted.chunks ?? [];
		let chunkIndex = 0;
		let aborted = false;
		const stream = new ReadableStream<Uint8Array>({
			pull(streamController) {
				if (aborted) {
					streamController.error(
						new DOMException("This operation was aborted", "AbortError"),
					);
					return;
				}
				if (chunkIndex >= chunks.length) {
					streamController.close();
					return;
				}
				const chunk = chunks[chunkIndex++]!;
				streamController.enqueue(encodeChunk(chunk));
				if (abortAfterChunks !== undefined && chunkIndex === abortAfterChunks) {
					controller.abort();
					aborted = true;
				}
			},
		});
		return new Response(stream, {
			status: scripted.status ?? 200,
			statusText: scripted.statusText ?? "",
			headers: {
				"content-type": "text/event-stream",
				...(scripted.headers ?? {}),
			},
		});
	}) as typeof fetch;
}

async function execute(request: RunRequest, api: ApiModule): Promise<unknown> {
	const controller = new AbortController();
	const requests: RecordedRequest[] = [];
	activeRequests = requests;
	activeTokenResponses = request.tokenResponses ?? [];
	setFetchScript(
		installFetch(
			request.responses,
			requests,
			controller,
			request.abortAfterChunks,
		),
	);
	try {
		const options: Record<string, unknown> = {
			...(request.stream ?? {}),
			...(request.extra ?? {}),
		};
		if (request.operation === "streamSimple") {
			if (request.reasoning !== undefined)
				options.reasoning = request.reasoning;
			if (request.thinkingBudgets !== undefined)
				options.thinkingBudgets = request.thinkingBudgets;
		}
		if (request.onPayloadPatch !== undefined) {
			options.onPayload = async (payload: unknown) => request.onPayloadPatch;
		}
		if (request.abortAfterChunks !== undefined || request.preAborted)
			options.signal = controller.signal;
		if (request.preAborted) controller.abort();

		const events: unknown[] = [];
		const runner =
			request.operation === "streamSimple" ? api.streamSimple : api.stream;
		const stream = runner(request.model, request.context, options);
		for await (const event of stream) {
			// Every queue entry is already cloned at push, including early setup errors.
			events.push(normalizeEvent(event));
		}
		const result = normalizeAssistantMessageClock(await stream.result());
		return { requests, events, result };
	} finally {
		clearFetchScript();
	}
}

async function loadApi(moduleDir: string, api: string): Promise<ApiModule> {
	const file =
		api === "google-vertex" ? "google-vertex.ts" : "google-generative-ai.ts";
	return (await import(
		pathToFileURL(path.join(moduleDir, file)).href
	)) as ApiModule;
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "Unknown", message: String(error) };
}

async function main(): Promise<void> {
	const moduleDir = process.argv[2];
	if (!moduleDir) throw new Error("module directory argument is required");
	installScriptedFetch();
	await installAssistantSnapshots(path.dirname(moduleDir));
	const apis = new Map<string, ApiModule>();
	const proxyPort = await startTokenProxy();
	process.env.HTTPS_PROXY = `http://127.0.0.1:${proxyPort}`;
	process.env.https_proxy = process.env.HTTPS_PROXY;

	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	process.stdin.on("data", (data: Buffer) => {
		buffer += data.toString("utf8");
		for (;;) {
			const newline = buffer.indexOf("\n");
			if (newline === -1) return;
			const line = buffer.slice(0, newline);
			buffer = buffer.slice(newline + 1);
			if (line.trim().length === 0) continue;
			queue = queue.then(() => handleLine(line));
		}
	});

	async function handleLine(line: string): Promise<void> {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as RunRequest;
			id = request.id;
			let api = apis.get(request.api);
			if (!api) {
				api = await loadApi(moduleDir, request.api);
				apis.set(request.api, api);
			}
			const output = await execute(request, api);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
			);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`google api oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
