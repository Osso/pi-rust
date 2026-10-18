import * as fs from "node:fs";
import * as net from "node:net";
import * as path from "node:path";
import * as tls from "node:tls";
import { pathToFileURL } from "node:url";

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
	mode: "stream" | "simple";
	model: unknown;
	context: unknown;
	options?: Record<string, unknown>;
	responses: ScriptedResponse[];
	tokenResponses?: ScriptedResponse[];
	abortAfterEvents?: number;
	onPayloadPatch?: unknown;
};

type ApiModule = {
	stream: (model: unknown, context: unknown, options?: unknown) => AsyncIterable<unknown> & {
		result(): Promise<unknown>;
	};
	streamSimple: (model: unknown, context: unknown, options?: unknown) => AsyncIterable<unknown> & {
		result(): Promise<unknown>;
	};
};

type RecordedRequest = {
	method: string;
	url: string;
	headers: Record<string, string>;
	body: string | null;
	jwt?: { header: unknown; claims: unknown; signatureLength: number } | null;
};

type NormalizedRun = {
	requests: RecordedRequest[];
	payloads: unknown[];
	events: unknown[];
	result: unknown;
};

const GENERATED_ID = /_1\d{12}_\d+$/;

function normalizeId(id: string): string {
	return id.replace(GENERATED_ID, "_<generated>");
}

function normalize(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(normalize);
	if (value && typeof value === "object") {
		const source = value as Record<string, unknown>;
		const result: Record<string, unknown> = {};
		for (const [key, entry] of Object.entries(source)) {
			if (key === "timestamp") result[key] = 0;
			else if (key === "id" && typeof entry === "string") result[key] = normalizeId(entry);
			else result[key] = normalize(entry);
		}
		return result;
	}
	return value;
}

function decodeBase64Url(text: string): string {
	return Buffer.from(text, "base64url").toString("utf8");
}

function normalizeBody(body: string | null): {
	body: string | null;
	jwt: RecordedRequest["jwt"];
} {
	if (body === null || !body.includes("assertion=")) return { body, jwt: null };
	const params = new URLSearchParams(body);
	const assertion = params.get("assertion");
	if (!assertion) return { body, jwt: null };
	const [header, claims, signature] = assertion.split(".");
	const normalized = {
		header: JSON.parse(decodeBase64Url(header)),
		claims: JSON.parse(decodeBase64Url(claims)),
		signatureLength: signature.length,
	} as { header: Record<string, unknown>; claims: Record<string, unknown>; signatureLength: number };
	normalized.claims.iat = 0;
	normalized.claims.exp = 0;
	return { body: body.replace(assertion, "<jwt>"), jwt: normalized };
}

function normalizeHeaders(headers: Headers): Record<string, string> {
	const result: Record<string, string> = {};
	for (const [name, value] of headers.entries()) {
		if (name === "user-agent" || name === "x-goog-api-client") result[name] = "<sdk-ua>";
		else result[name] = value;
	}
	return result;
}

function encodeChunk(chunk: ChunkSpec): Uint8Array {
	if (typeof chunk === "string") return new TextEncoder().encode(chunk);
	return new Uint8Array(Buffer.from(chunk.base64, "base64"));
}

function scriptedResponse(scripted: ScriptedResponse): Response {
	const status = scripted.status ?? 200;
	const headers = new Headers(scripted.headers ?? {});
	const chunks = scripted.chunks ?? [];
	const stream = new ReadableStream<Uint8Array>({
		start(controller) {
			for (const chunk of chunks) controller.enqueue(encodeChunk(chunk));
			controller.close();
		},
	});
	return new Response(stream, { status, statusText: scripted.statusText ?? "", headers });
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
			const requestLine = handshake.subarray(0, headerEnd).toString("utf8").split("\r\n")[0] ?? "";
			const target = requestLine.split(" ")[1] ?? "oauth2.googleapis.com:443";
			socket.write("HTTP/1.1 200 Connection Established\r\n\r\n");
			const tlsSocket = new tls.TLSSocket(socket, { isServer: true, secureContext });
			serveTokenRequest(tlsSocket, target.split(":")[0] ?? "oauth2.googleapis.com");
		};
		socket.on("data", onHandshake);
		socket.on("error", () => {});
	});
	return new Promise((resolve) => {
		server.listen(0, "127.0.0.1", () => {
			const address = server.address() as net.AddressInfo;
			resolve(address.port);
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
			if (separator > 0) headers[line.slice(0, separator).toLowerCase()] = line.slice(separator + 1).trim();
		}
		const length = Number(headers["content-length"] ?? "0");
		const bodyStart = headerEnd + 4;
		if (buffer.length < bodyStart + length) return;
		socket.off("data", onData);

		const body = buffer.subarray(bodyStart, bodyStart + length).toString("utf8");
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
			.map((chunk) => (typeof chunk === "string" ? chunk : Buffer.from(chunk.base64, "base64").toString("utf8")))
			.join("");
		const responseHeaders = { "content-type": "application/json", ...(scripted?.headers ?? {}) };
		const head = [
			`HTTP/1.1 ${status} ${scripted?.statusText ?? "OK"}`,
			...Object.entries(responseHeaders).map(([name, value]) => `${name}: ${value}`),
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
): typeof fetch {
	return (async (input: unknown, init: Record<string, unknown> = {}) => {
		const url = typeof input === "string" ? input : String((input as { url: string }).url);
		const method = (init.method as string | undefined) ?? "GET";
		const headers = new Headers(init.headers as HeadersInit | undefined);
		const rawBody = init.body as string | URLSearchParams | undefined;
		let body: string | null = null;
		if (rawBody instanceof URLSearchParams) {
			body = rawBody.toString();
			if (!headers.has("content-type")) {
				headers.set("content-type", "application/x-www-form-urlencoded;charset=UTF-8");
			}
		} else if (typeof rawBody === "string") {
			body = rawBody;
		}
		const normalized = normalizeBody(body);
		const request: RecordedRequest = {
			method,
			url,
			headers: normalizeHeaders(headers),
			body: normalized.body,
		};
		if (normalized.jwt) request.jwt = normalized.jwt;
		requests.push(request);

		const scripted = responses.shift();
		if (!scripted) throw new Error("unexpected fetch call");
		if (scripted.throw) {
			const error = new Error(scripted.throw.message);
			if (scripted.throw.name) error.name = scripted.throw.name;
			throw error;
		}
		return scriptedResponse(scripted);
	}) as typeof fetch;
}

async function execute(request: RunRequest, api: ApiModule, controller: AbortController): Promise<NormalizedRun> {
	const requests: RecordedRequest[] = [];
	const payloads: unknown[] = [];
	activeRequests = requests;
	activeTokenResponses = request.tokenResponses ?? [];
	globalThis.fetch = installFetch(request.responses, requests);

	const options: Record<string, unknown> = { ...(request.options ?? {}) };
	if (request.onPayloadPatch !== undefined) {
		options.onPayload = async (payload: unknown) => {
			payloads.push(JSON.parse(JSON.stringify(payload)));
			return request.onPayloadPatch;
		};
	}
	if (request.abortAfterEvents !== undefined) options.signal = controller.signal;

	const runner = request.mode === "simple" ? api.streamSimple : api.stream;
	const events: unknown[] = [];
	let result: unknown = null;
	let thrown: { name: string; message: string } | null = null;
	try {
		const stream = runner(request.model, request.context, options);
		for await (const event of stream) {
			events.push(JSON.parse(JSON.stringify(normalize(event))));
			if (request.abortAfterEvents !== undefined && events.length === request.abortAfterEvents) {
				controller.abort();
			}
		}
		result = normalize(await stream.result());
	} catch (error) {
		thrown = error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
	}
	if (thrown) {
		return { requests, payloads: normalize(payloads), events, result: thrown };
	}
	return { requests, payloads: normalize(payloads), events, result };
}

async function loadApi(moduleDir: string, api: string): Promise<ApiModule> {
	const file = api === "google-vertex" ? "google-vertex.ts" : "google-generative-ai.ts";
	return (await import(pathToFileURL(path.join(moduleDir, file)).href)) as ApiModule;
}

async function main(): Promise<void> {
	const moduleDir = process.argv[2];
	if (!moduleDir) throw new Error("module directory argument is required");
	const apis = new Map<string, ApiModule>();
	const proxyPort = await startTokenProxy();
	process.env.HTTPS_PROXY = `http://127.0.0.1:${proxyPort}`;
	process.env.https_proxy = process.env.HTTPS_PROXY;
	const decoder = new TextDecoder();
	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	process.stdin.on("data", (data: Buffer) => {
		buffer += decoder.decode(data, { stream: true });
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
			const output = await execute(request, api, new AbortController());
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			const normalized =
				error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalized })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`google api oracle startup failed: ${error instanceof Error ? error.message : String(error)}\n`);
	process.exitCode = 1;
});
