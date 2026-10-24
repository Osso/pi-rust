import crypto from "node:crypto";
import net from "node:net";
import { dirname } from "node:path";
import { pathToFileURL } from "node:url";
import { zstdDecompressSync } from "node:zlib";
import { describeAbortReason } from "./abort-reason-snapshot.ts";
import {
	installAssistantSnapshots,
	normalizeAssistantMessageClock as normalize,
	normalizeAssistantEventClocks as normalizeEvent,
} from "./assistant-snapshots.ts";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

type RequestId = string | number | null;

interface SseRequest {
	id: RequestId;
	operation: "sse";
	events: unknown[];
	modelOverrides?: Record<string, unknown>;
	options?: Record<string, unknown>;
	onPayloadError?: string;
	onPayloadRejection?: unknown;
	onPayloadReplacement?: unknown;
	onResponseError?: string;
	onResponseRejection?: unknown;
	onRetryError?: string;
	onRetryRejection?: unknown;
	context?: unknown;
	rawPayload?: string;
	retryStatus?: number;
	retryHeaders?: Array<[string, string]>;
	responseHeaders?: Array<[string, string]>;
	retryBody?: string;
	maxRetries?: number;
	timeoutMs?: number;
	pendingBody?: boolean;
	pendingHeaders?: boolean;
	responseDelayMs?: number;
	abortAfterDelta?: boolean;
	chunkSize?: number;
	observeSignals?: boolean;
	omitSignal?: boolean;
	abortAfterCompletion?: boolean;
}

interface WsRequest {
	id: RequestId;
	operation:
		| "ws_cached"
		| "ws_idle"
		| "ws_connect_timeout"
		| "ws_accounts"
		| "ws_events"
		| "ws_age"
		| "ws_busy"
		| "ws_inputs";
	onPayloadReplacement?: unknown;
	firstInput?: unknown;
	secondInput?: unknown;
	wsUrl: string;
	timeoutMs?: number;
	connectTimeoutMs?: number;
	reorderSecondPayload?: "body" | "input";
	secondAccountId?: string;
	cacheRetention?: "none";
}

type Request = SseRequest | WsRequest;

type Api = {
	stream: (
		model: unknown,
		context: unknown,
		options: unknown,
	) => AsyncIterable<unknown> & {
		result: () => Promise<unknown>;
	};
	getOpenAICodexWebSocketDebugStats: (sessionId: string) => unknown;
	closeOpenAICodexWebSocketSessions: (sessionId?: string) => void;
	resetOpenAICodexWebSocketDebugStats: (sessionId?: string) => void;
	streamSimple: (
		model: unknown,
		context: unknown,
		options: unknown,
	) => AsyncIterable<unknown> & {
		result: () => Promise<unknown>;
	};
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function mockToken(accountId = "acc_test"): string {
	const payload = Buffer.from(
		JSON.stringify({
			"https://api.openai.com/auth": { chatgpt_account_id: accountId },
		}),
		"utf8",
	).toString("base64");
	return `aaa.${payload}.bbb`;
}

function model(
	baseUrl = "https://chatgpt.com/backend-api",
): Record<string, unknown> {
	return {
		id: "gpt-5.1-codex",
		name: "GPT-5.1 Codex",
		api: "openai-codex-responses",
		provider: "openai-codex",
		baseUrl,
		reasoning: true,
		input: ["text"],
		cost: { input: 1, output: 2, cacheRead: 0.5, cacheWrite: 0.25 },
		contextWindow: 400000,
		maxTokens: 128000,
	};
}

function context(): Record<string, unknown> {
	return {
		systemPrompt: "You are a helpful assistant.",
		messages: [{ role: "user", content: "Say hello", timestamp: 1 }],
	};
}

function decodeBody(body: unknown, headers: Record<string, string>): unknown {
	if (typeof body === "string") return JSON.parse(body);
	if (body instanceof Uint8Array) {
		const bytes =
			headers["content-encoding"] === "zstd" ? zstdDecompressSync(body) : body;
		return JSON.parse(Buffer.from(bytes).toString("utf8"));
	}
	return null;
}

interface RecordedRequest {
	url: string;
	method: string;
	headers: Record<string, string>;
	body: unknown;
}

function installFetch(
	request: SseRequest,
	recorded: RecordedRequest[],
	signals: Array<AbortSignal | undefined>,
): void {
	let calls = 0;
	setFetchScript((async (input: string | URL, init?: RequestInit) => {
		signals.push(init?.signal ?? undefined);
		const url = typeof input === "string" ? input : input.toString();
		const headers =
			init?.headers instanceof Headers
				? Object.fromEntries(init.headers.entries())
				: {};
		recorded.push({
			url,
			method: init?.method ?? "GET",
			headers,
			body: decodeBody(init?.body, headers),
		});
		if (request.responseDelayMs !== undefined) {
			await new Promise((resolve) =>
				setTimeout(resolve, request.responseDelayMs),
			);
		}
		if (request.pendingHeaders) {
			return new Promise<Response>((_resolve, reject) => {
				const signal = init?.signal;
				const onAbort = () =>
					reject(
						signal?.reason instanceof Error
							? signal.reason
							: new Error("aborted"),
					);
				if (signal?.aborted) onAbort();
				else signal?.addEventListener("abort", onAbort, { once: true });
			});
		}
		calls += 1;
		if (request.retryStatus !== undefined && calls === 1) {
			return new Response(request.retryBody ?? "error", {
				status: request.retryStatus,
				headers: request.retryHeaders ?? [],
			});
		}
		const payload =
			request.rawPayload ??
			request.events
				.map((event) => `data: ${JSON.stringify(event)}\n\n`)
				.join("");
		const chunks: string[] = [];
		const size = Math.max(1, request.chunkSize ?? payload.length);
		for (let index = 0; index < payload.length; index += size) {
			chunks.push(payload.slice(index, index + size));
		}
		const encoder = new TextEncoder();
		const stream = new ReadableStream<Uint8Array>({
			start(controller) {
				for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
				if (!request.pendingBody) controller.close();
			},
		});
		return new Response(stream, {
			status: 200,
			headers: [
				["content-type", "text/event-stream"],
				...(request.responseHeaders ?? []),
			],
		});
	}) as typeof fetch);
}

async function runSse(api: Api, request: SseRequest): Promise<unknown> {
	const recorded: RecordedRequest[] = [];
	const signals: Array<AbortSignal | undefined> = [];
	installFetch(request, recorded, signals);
	const token = mockToken();
	const controller = new AbortController();
	const retries: unknown[] = [];
	const responseStatuses: number[] = [];
	const options: Record<string, unknown> = {
		apiKey: token,
		transport: "sse",
		sessionId: "diff-session",
		onRetry: (event: unknown) => {
			retries.push(structuredClone(event));
			if ("onRetryRejection" in request && retries.length === 1)
				throw request.onRetryRejection;
			if (request.onRetryError !== undefined && retries.length === 1)
				throw new Error(request.onRetryError);
		},
		onResponse: (event: { status: number }) => {
			responseStatuses.push(event.status);
			if ("onResponseRejection" in request && responseStatuses.length === 1)
				throw request.onResponseRejection;
			if (
				request.onResponseError !== undefined &&
				responseStatuses.length === 1
			)
				throw new Error(request.onResponseError);
		},
	};
	if (request.onPayloadError !== undefined) {
		options.onPayload = () => {
			throw new Error(request.onPayloadError);
		};
	}
	if ("onPayloadRejection" in request) {
		options.onPayload = () => {
			throw request.onPayloadRejection;
		};
	}
	if ("onPayloadReplacement" in request)
		options.onPayload = () => request.onPayloadReplacement;
	const suppliedSignal = request.omitSignal ? undefined : controller.signal;
	if (suppliedSignal) options.signal = suppliedSignal;
	if (request.maxRetries !== undefined) options.maxRetries = request.maxRetries;
	if (request.timeoutMs !== undefined) options.timeoutMs = request.timeoutMs;
	Object.assign(options, request.options);

	const resultStream = api.stream(
		{ ...model(), ...request.modelOverrides },
		request.context ?? context(),
		options,
	);
	const events: unknown[] = [];
	for await (const event of resultStream) {
		events.push(normalizeEvent(event));
		if (
			request.abortAfterDelta &&
			(event as { type?: string }).type === "text_delta"
		) {
			controller.abort();
		}
	}
	const result = await resultStream.result();
	if (request.abortAfterCompletion) controller.abort("finished\ud800");
	return {
		requests: recorded,
		events,
		result: normalize(result),
		retries,
		responseStatuses,
		...(request.observeSignals
			? {
					fetchSignals: signals.map((signal) => ({
						sameAsCaller:
							suppliedSignal !== undefined && signal === suppliedSignal,
						state: signal
							? {
									aborted: signal.aborted,
									reason: describeAbortReason(signal.reason),
								}
							: { kind: "no-signal" },
					})),
				}
			: {}),
	};
}

class MinimalWebSocket {
	static OPEN = 1;
	readyState = 0;
	private listeners = new Map<string, Set<(event: unknown) => void>>();
	private socket: net.Socket;
	private buffer = Buffer.alloc(0);
	private handshaken = false;

	constructor(
		url: string,
		protocols?: string | string[] | { headers?: Record<string, string> },
	) {
		const parsed = new URL(url);
		const headers =
			protocols && typeof protocols === "object" && !Array.isArray(protocols)
				? (protocols.headers ?? {})
				: {};
		this.socket = net.connect(
			Number(parsed.port || 80),
			parsed.hostname,
			() => {
				const key = crypto.randomBytes(16).toString("base64");
				const lines = [
					`GET ${parsed.pathname}${parsed.search} HTTP/1.1`,
					`Host: ${parsed.host}`,
					"Upgrade: websocket",
					"Connection: Upgrade",
					`Sec-WebSocket-Key: ${key}`,
					"Sec-WebSocket-Version: 13",
				];
				for (const [name, value] of Object.entries(headers))
					lines.push(`${name}: ${value}`);
				this.socket.write(`${lines.join("\r\n")}\r\n\r\n`);
			},
		);
		this.socket.on("data", (data) => this.onData(data));
		this.socket.on("error", (error) =>
			this.dispatch("error", { message: error.message }),
		);
		this.socket.on("close", () => {
			this.readyState = 3;
			this.dispatch("close", { code: 1006, reason: "", wasClean: false });
		});
	}

	addEventListener(type: string, listener: (event: unknown) => void): void {
		let listeners = this.listeners.get(type);
		if (!listeners) {
			listeners = new Set();
			this.listeners.set(type, listeners);
		}
		listeners.add(listener);
	}

	removeEventListener(type: string, listener: (event: unknown) => void): void {
		this.listeners.get(type)?.delete(listener);
	}

	send(data: string): void {
		this.sendFrame(1, Buffer.from(data));
	}

	close(code = 1000, reason = ""): void {
		const reasonBytes = Buffer.from(reason);
		const payload = Buffer.alloc(2 + reasonBytes.length);
		payload.writeUInt16BE(code, 0);
		reasonBytes.copy(payload, 2);
		this.sendFrame(8, payload);
		this.socket.end();
		this.readyState = 3;
	}

	private dispatch(type: string, event: unknown): void {
		for (const listener of this.listeners.get(type) ?? []) listener(event);
	}

	private onData(data: Buffer): void {
		this.buffer = Buffer.concat([this.buffer, data]);
		if (!this.handshaken) {
			const end = this.buffer.indexOf("\r\n\r\n");
			if (end === -1) return;
			this.handshaken = true;
			this.buffer = this.buffer.subarray(end + 4);
			queueMicrotask(() => {
				this.readyState = 1;
				this.dispatch("open", {});
			});
		}
		while (true) {
			if (this.buffer.length < 2) return;
			const first = this.buffer[0];
			const second = this.buffer[1];
			const opcode = first & 0x0f;
			const masked = (second & 0x80) !== 0;
			let length = second & 0x7f;
			let offset = 2;
			if (length === 126) {
				if (this.buffer.length < 4) return;
				length = this.buffer.readUInt16BE(2);
				offset = 4;
			} else if (length === 127) {
				if (this.buffer.length < 10) return;
				length = Number(this.buffer.readBigUInt64BE(2));
				offset = 10;
			}
			const maskLength = masked ? 4 : 0;
			if (this.buffer.length < offset + maskLength + length) return;
			let payload = this.buffer.subarray(
				offset + maskLength,
				offset + maskLength + length,
			);
			if (masked) {
				const mask = this.buffer.subarray(offset, offset + 4);
				payload = Buffer.from(
					payload.map((byte, index) => byte ^ mask[index % 4]),
				);
			}
			this.buffer = this.buffer.subarray(offset + maskLength + length);
			if (opcode === 1) {
				this.dispatch("message", { data: payload.toString("utf8") });
			} else if (opcode === 8) {
				const code = payload.length >= 2 ? payload.readUInt16BE(0) : 1005;
				const reason =
					payload.length > 2 ? payload.subarray(2).toString("utf8") : "";
				this.readyState = 3;
				this.dispatch("close", { code, reason, wasClean: true });
			} else if (opcode === 9) {
				this.sendFrame(10, payload);
			}
		}
	}

	private sendFrame(opcode: number, payload: Buffer): void {
		const mask = crypto.randomBytes(4);
		const length = payload.length;
		let header: Buffer;
		if (length < 126) {
			header = Buffer.from([0x80 | opcode, 0x80 | length]);
		} else if (length < 65536) {
			header = Buffer.alloc(4);
			header[0] = 0x80 | opcode;
			header[1] = 0x80 | 126;
			header.writeUInt16BE(length, 2);
		} else {
			header = Buffer.alloc(10);
			header[0] = 0x80 | opcode;
			header[1] = 0x80 | 127;
			header.writeBigUInt64BE(BigInt(length), 2);
		}
		const masked = Buffer.from(
			payload.map((byte, index) => byte ^ mask[index % 4]),
		);
		this.socket.write(Buffer.concat([header, mask, masked]));
	}
}

function reverseObjectProperties(
	value: Record<string, unknown>,
): Record<string, unknown> {
	return Object.fromEntries(Object.entries(value).reverse());
}

function installCacheExpiryClock(): {
	advance: (milliseconds: number) => void;
	restore: () => void;
} {
	const originalSetTimeout = globalThis.setTimeout;
	const originalClearTimeout = globalThis.clearTimeout;
	const timers = new Map<
		ReturnType<typeof setTimeout>,
		{ due: number; fire: () => void }
	>();
	let now = 0;
	globalThis.setTimeout = ((
		callback: (...args: unknown[]) => void,
		delay?: number,
		...args: unknown[]
	) => {
		if (delay !== 300000)
			return originalSetTimeout(() => callback(...args), delay);
		const handle = originalSetTimeout(() => {}, delay);
		timers.set(handle, { due: now + delay, fire: () => callback(...args) });
		return handle;
	}) as typeof setTimeout;
	globalThis.clearTimeout = ((handle: ReturnType<typeof setTimeout>) => {
		timers.delete(handle);
		originalClearTimeout(handle);
	}) as typeof clearTimeout;
	return {
		advance(milliseconds) {
			now += milliseconds;
			for (const [handle, timer] of [...timers]) {
				if (timer.due > now) continue;
				timers.delete(handle);
				originalClearTimeout(handle);
				timer.fire();
			}
		},
		restore() {
			for (const handle of timers.keys()) originalClearTimeout(handle);
			globalThis.setTimeout = originalSetTimeout;
			globalThis.clearTimeout = originalClearTimeout;
		},
	};
}

async function runAccountLifecycle(
	api: Api,
	request: WsRequest,
): Promise<unknown> {
	const session = "diff-ws-accounts";
	const clock = installCacheExpiryClock();
	const outputs: unknown[] = [];
	try {
		for (const step of [
			"acc_a",
			150000,
			"acc_b",
			"acc_a",
			150000,
			"acc_b",
			150000,
			"acc_b",
			"acc_a",
			300000,
			"acc_b",
			"acc_a",
			"close",
			"acc_b",
			"acc_a",
		] as const) {
			if (typeof step === "number") {
				clock.advance(step);
				continue;
			}
			if (step === "close") {
				api.closeOpenAICodexWebSocketSessions(session);
				continue;
			}
			const stream = api.stream(model(request.wsUrl), context(), {
				apiKey: mockToken(step),
				transport: "websocket-cached",
				sessionId: session,
			});
			const events: unknown[] = [];
			for await (const event of stream) events.push(normalizeEvent(event));
			outputs.push({
				events,
				result: normalize(await stream.result()),
				stats: api.getOpenAICodexWebSocketDebugStats(session),
			});
		}
		api.closeOpenAICodexWebSocketSessions(session);
		return outputs;
	} finally {
		api.closeOpenAICodexWebSocketSessions(session);
		api.resetOpenAICodexWebSocketDebugStats(session);
		clock.restore();
	}
}

async function runAgeLifecycle(api: Api, request: WsRequest): Promise<unknown> {
	const session = "diff-ws-age";
	const originalNow = Date.now;
	let offset = 0;
	let sent = 0;
	Date.now = () => originalNow() + offset;
	(globalThis as { WebSocket?: unknown }).WebSocket = class extends (
		MinimalWebSocket
	) {
		override send(data: string): void {
			super.send(data);
			if (++sent === 2) offset = 55 * 60 * 1000;
		}
	};
	try {
		const outputs: unknown[] = [];
		for (let call = 0; call < 3; call++) {
			const stream = api.stream(model(request.wsUrl), context(), {
				apiKey: mockToken(),
				transport: "websocket-cached",
				sessionId: session,
			});
			const events: unknown[] = [];
			for await (const event of stream) events.push(normalizeEvent(event));
			outputs.push({
				events,
				result: normalize(await stream.result()),
				stats: api.getOpenAICodexWebSocketDebugStats(session),
			});
		}
		return outputs;
	} finally {
		api.closeOpenAICodexWebSocketSessions(session);
		api.resetOpenAICodexWebSocketDebugStats(session);
		Date.now = originalNow;
		(globalThis as { WebSocket?: unknown }).WebSocket = MinimalWebSocket;
	}
}

async function runConcurrentLifecycle(
	api: Api,
	request: WsRequest,
): Promise<unknown> {
	const session = "diff-ws-busy";
	let sends = 0;
	const firstSent = new Promise<void>((resolve) => {
		(globalThis as { WebSocket?: unknown }).WebSocket = class extends (
			MinimalWebSocket
		) {
			override send(data: string): void {
				super.send(data);
				if (++sends === 1) resolve();
			}
		};
	});
	const collect = async (stream: ReturnType<Api["stream"]>) => {
		const events: unknown[] = [];
		for await (const event of stream) events.push(normalizeEvent(event));
		return { events, result: normalize(await stream.result()) };
	};
	const call = () =>
		api.stream(model(request.wsUrl), context(), {
			apiKey: mockToken(),
			transport: "websocket-cached",
			sessionId: session,
		});
	try {
		const first = call();
		await firstSent;
		const second = await collect(call());
		const firstOutput = await collect(first);
		const third = await collect(call());
		return {
			outputs: [firstOutput, second, third],
			stats: api.getOpenAICodexWebSocketDebugStats(session),
		};
	} finally {
		api.closeOpenAICodexWebSocketSessions(session);
		api.resetOpenAICodexWebSocketDebugStats(session);
		(globalThis as { WebSocket?: unknown }).WebSocket = MinimalWebSocket;
	}
}

async function runInputLifecycle(
	api: Api,
	request: WsRequest,
): Promise<unknown> {
	const session = "diff-ws-inputs";
	try {
		const outputs: unknown[] = [];
		for (const field of ["firstInput", "secondInput"] as const) {
			const before = Date.now();
			const stream = api.stream(model(request.wsUrl), context(), {
				apiKey: mockToken(),
				transport: "websocket-cached",
				sessionId: session,
				onPayload: (payload: Record<string, unknown>) => {
					const body = { ...payload };
					if (field in request) body.input = request[field];
					else delete body.input;
					return body;
				},
			});
			const events: unknown[] = [];
			for await (const event of stream) events.push(normalizeEvent(event));
			outputs.push({
				events,
				result: normalize(await stream.result()),
				clockBounds: [before, Date.now()],
			});
		}
		return { outputs, stats: api.getOpenAICodexWebSocketDebugStats(session) };
	} finally {
		api.closeOpenAICodexWebSocketSessions(session);
		api.resetOpenAICodexWebSocketDebugStats(session);
	}
}

async function runWs(api: Api, request: WsRequest): Promise<unknown> {
	const before = Date.now();
	(globalThis as { WebSocket?: unknown }).WebSocket = MinimalWebSocket;
	if (request.operation === "ws_accounts")
		return runAccountLifecycle(api, request);
	if (request.operation === "ws_age") return runAgeLifecycle(api, request);
	if (request.operation === "ws_busy")
		return runConcurrentLifecycle(api, request);
	if (request.operation === "ws_inputs") return runInputLifecycle(api, request);
	const token = mockToken();
	const options = (sessionId: string, extra: Record<string, unknown> = {}) => ({
		apiKey: token,
		transport: "websocket-cached",
		sessionId,
		cacheRetention: request.cacheRetention,
		...extra,
	});
	const baseUrl = request.wsUrl;
	const baseModel = model(baseUrl);

	if (request.operation === "ws_idle") {
		const resultStream = api.stream(
			baseModel,
			context(),
			options("diff-ws-idle", {
				transport: "websocket",
				timeoutMs: request.timeoutMs ?? 50,
			}),
		);
		const events: unknown[] = [];
		for await (const event of resultStream) events.push(normalizeEvent(event));
		return {
			events,
			result: normalize(await resultStream.result()),
			clockBounds: [before, Date.now()],
		};
	}
	if (request.operation === "ws_connect_timeout") {
		const resultStream = api.stream(
			baseModel,
			context(),
			options("diff-ws-connect", {
				transport: "websocket",
				websocketConnectTimeoutMs: request.connectTimeoutMs ?? 50,
			}),
		);
		const events: unknown[] = [];
		for await (const event of resultStream) events.push(normalizeEvent(event));
		return {
			events,
			result: normalize(await resultStream.result()),
			clockBounds: [before, Date.now()],
		};
	}

	if (request.operation === "ws_events") {
		const session = "diff-ws-events";
		try {
			const extra =
				"onPayloadReplacement" in request
					? { onPayload: () => request.onPayloadReplacement }
					: {};
			const stream = api.stream(baseModel, context(), options(session, extra));
			const events: unknown[] = [];
			for await (const event of stream) events.push(normalizeEvent(event));
			return {
				events,
				result: normalize(await stream.result()),
				clockBounds: [before, Date.now()],
				stats: api.getOpenAICodexWebSocketDebugStats(session),
			};
		} finally {
			api.closeOpenAICodexWebSocketSessions(session);
			api.resetOpenAICodexWebSocketDebugStats(session);
		}
	}
	const firstContext = context();
	const firstStream = api.stream(
		baseModel,
		firstContext,
		options("diff-ws-cached"),
	);
	const firstEvents: unknown[] = [];
	for await (const event of firstStream)
		firstEvents.push(normalizeEvent(event));
	const first = await firstStream.result();

	const secondContext = {
		...firstContext,
		messages: [
			...(firstContext.messages as unknown[]),
			first,
			{ role: "user", content: "Now finish", timestamp: 2 },
		],
	};
	const secondStream = api.stream(
		baseModel,
		secondContext,
		options("diff-ws-cached", {
			apiKey: request.secondAccountId
				? mockToken(request.secondAccountId)
				: token,
			onPayload: (payload: Record<string, unknown>) => {
				if (request.reorderSecondPayload === "body") {
					return reverseObjectProperties(payload);
				}
				if (request.reorderSecondPayload === "input") {
					const input = payload.input as Record<string, unknown>[];
					return {
						...payload,
						input: [reverseObjectProperties(input[0]), ...input.slice(1)],
					};
				}
				return payload;
			},
		}),
	);
	const secondEvents: unknown[] = [];
	for await (const event of secondStream)
		secondEvents.push(normalizeEvent(event));
	return {
		firstEvents,
		first: normalize(first),
		secondEvents,
		second: normalize(await secondStream.result()),
	};
}

async function execute(api: Api, request: Request): Promise<unknown> {
	try {
		if (request.operation === "sse") return await runSse(api, request);
		return await runWs(api, request);
	} finally {
		clearFetchScript();
	}
}

function serveRequests(api: Api): void {
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(async () => {
				let id: RequestId = null;
				try {
					const request = JSON.parse(line) as Request;
					id = request.id;
					const output = await execute(api, request);
					process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
				} catch (error) {
					process.stdout.write(
						`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
					);
				}
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("codex oracle requires a source module path");
	installScriptedFetch();
	// This oracle exclusively proves the approved per-account WebSocket cache delta.
	process.env.PI_APPROVED_DELTA = "1";
	const { selectApprovedModule } = await import("./approved-delta.mjs");
	const codexPath = selectApprovedModule(
		`${modulePath}/api/openai-codex-responses.ts`,
		"packages/ai/src/api/openai-codex-responses.ts",
	);
	await installAssistantSnapshots(dirname(dirname(codexPath)));
	const codex = (await import(pathToFileURL(codexPath).href)) as Api;
	serveRequests(codex);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`codex oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
