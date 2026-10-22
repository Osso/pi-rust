import { pathToFileURL } from "node:url";
import { Readable } from "node:stream";
import { crc32 } from "node:zlib";
import { installAssistantSnapshots } from "./assistant-snapshots.ts";

type RequestId = string | number | null;

interface FrameSpec {
	kind: "event" | "exception";
	type: string;
	payload: unknown;
	corruptPrelude?: boolean;
	corruptMessage?: boolean;
}

interface ResponseSpec {
	status: number;
	headers: Array<[string, string]>;
	frames?: FrameSpec[];
	text?: string;
	chunkSizes?: number[];
}

interface OracleRequest {
	id: RequestId;
	model: Record<string, unknown>;
	context: Record<string, unknown>;
	options: Record<string, unknown>;
	processEnv?: Record<string, string>;
	script: ResponseSpec[];
	capturePayload?: boolean;
	captureResponse?: boolean;
	abort?: "immediate" | { afterEvents: number };
}

interface OracleResponse {
	id: RequestId;
	ok: true;
	output: unknown;
}

interface OracleError {
	id: RequestId;
	ok: false;
	error: { name: string; message: string };
}

const FIXED_MILLIS = Date.UTC(2026, 0, 2, 3, 4, 5, 0);
const RealDate = Date;
class FakeDate extends RealDate {
	constructor(...args: ConstructorParameters<typeof Date>) {
		if (args.length === 0) super(FIXED_MILLIS);
		else super(...args);
	}
	static now(): number {
		return FIXED_MILLIS;
	}
}
(globalThis as unknown as { Date: typeof Date }).Date =
	FakeDate as unknown as typeof Date;
(
	globalThis as unknown as { crypto: { randomUUID: () => string } }
).crypto.randomUUID = () => "11111111-2222-4333-8444-555555555555";
process.env.AWS_BEDROCK_FORCE_HTTP1 = "1";

function encodeFrame(
	headers: Array<[string, string]>,
	payload: Uint8Array,
	corruptPrelude: boolean,
	corruptMessage: boolean,
): Uint8Array {
	const headerParts: number[] = [];
	for (const [name, value] of headers) {
		const nameBytes = Buffer.from(name, "utf8");
		const valueBytes = Buffer.from(value, "utf8");
		headerParts.push(
			nameBytes.length,
			...nameBytes,
			7,
			valueBytes.length >> 8,
			valueBytes.length & 0xff,
			...valueBytes,
		);
	}
	const headersLength = headerParts.length;
	const totalLength = 12 + headersLength + payload.length + 4;
	const out = Buffer.alloc(totalLength);
	out.writeUInt32BE(totalLength, 0);
	out.writeUInt32BE(headersLength, 4);
	const preludeCrc =
		(crc32(out.subarray(0, 8)) ^ (corruptPrelude ? 1 : 0)) >>> 0;
	out.writeUInt32BE(preludeCrc, 8);
	Buffer.from(headerParts).copy(out, 12);
	Buffer.from(payload).copy(out, 12 + headersLength);
	const messageCrc =
		(crc32(out.subarray(0, totalLength - 4)) ^ (corruptMessage ? 1 : 0)) >>> 0;
	out.writeUInt32BE(messageCrc, totalLength - 4);
	return out;
}

function encodeFrameSpec(spec: FrameSpec): Uint8Array {
	const headers: Array<[string, string]> = [
		[":message-type", spec.kind === "event" ? "event" : "exception"],
		[spec.kind === "event" ? ":event-type" : ":exception-type", spec.type],
		[":content-type", "application/json"],
	];
	return encodeFrame(
		headers,
		Buffer.from(JSON.stringify(spec.payload), "utf8"),
		spec.corruptPrelude ?? false,
		spec.corruptMessage ?? false,
	);
}

function splitBytes(bytes: Uint8Array, sizes: number[] | undefined): Buffer[] {
	const body = Buffer.from(bytes);
	if (!sizes || sizes.length === 0) return [body];
	const chunks: Buffer[] = [];
	let index = 0;
	for (const size of sizes) {
		if (index >= body.length) break;
		const end = Math.min(index + size, body.length);
		chunks.push(body.subarray(index, end));
		index = end;
	}
	if (index < body.length) chunks.push(body.subarray(index));
	return chunks;
}

let activeScript: ResponseSpec[] = [];
let activeRequests: Array<Record<string, unknown>> = [];
let scriptIndex = 0;

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("bedrock oracle requires a source module path");
	await installAssistantSnapshots(modulePath);
	const sourceRoot = new URL("../../../", pathToFileURL(`${modulePath}/`));
	const nodeHttpHandler = (await import(
		new URL(
			"node_modules/@smithy/node-http-handler/dist-cjs/index.js",
			sourceRoot,
		).href
	)) as {
		NodeHttpHandler: {
			prototype: { handle: (...args: unknown[]) => Promise<unknown> };
		};
	};

	nodeHttpHandler.NodeHttpHandler.prototype.handle = async (
		request: unknown,
		options?: { abortSignal?: AbortSignal },
	) => {
		activeRequests.push(request as Record<string, unknown>);
		const response =
			activeScript[Math.min(scriptIndex, activeScript.length - 1)];
		scriptIndex += 1;
		if (!response) throw new Error("bedrock oracle script exhausted");
		const bytes =
			response.frames && response.frames.length > 0
				? Buffer.concat(response.frames.map(encodeFrameSpec))
				: Buffer.from(response.text ?? "", "utf8");
		const headers: Record<string, string> = {};
		for (const [name, value] of response.headers) headers[name] = value;
		const body = Readable.from(splitBytes(bytes, response.chunkSizes));
		const signal = options?.abortSignal;
		if (signal) {
			const abort = () => body.destroy(new Error("This operation was aborted"));
			if (signal.aborted) abort();
			else signal.addEventListener("abort", abort, { once: true });
		}
		return {
			response: {
				statusCode: response.status,
				headers,
				body,
			},
		};
	};

	const bedrock = (await import(
		pathToFileURL(`${modulePath}/api/bedrock-converse-stream.ts`).href
	)) as {
		stream: (
			model: unknown,
			context: unknown,
			options: unknown,
		) => AsyncIterable<unknown> & {
			result: () => Promise<unknown>;
		};
	};

	serveRequests(bedrock);
}

function restoreEnv(originals: Map<string, string | undefined>): void {
	for (const [name, value] of originals) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
}

async function execute(
	request: OracleRequest,
	bedrock: {
		stream: (
			model: unknown,
			context: unknown,
			options: unknown,
		) => AsyncIterable<unknown> & { result: () => Promise<unknown> };
	},
): Promise<OracleResponse> {
	activeScript = request.script;
	activeRequests = [];
	scriptIndex = 0;
	const originals = new Map<string, string | undefined>();
	for (const [name, value] of Object.entries(request.processEnv ?? {})) {
		originals.set(name, process.env[name]);
		process.env[name] = value;
	}
	const controller = new AbortController();
	const options: Record<string, unknown> = { ...request.options };
	options.signal = controller.signal;
	let capturedPayload: unknown;
	let capturedResponse: unknown;
	if (request.capturePayload) {
		options.onPayload = (payload: unknown) => {
			capturedPayload = payload;
		};
	}
	if (request.captureResponse) {
		options.onResponse = (response: unknown) => {
			capturedResponse = response;
		};
	}
	if (request.abort === "immediate") controller.abort();

	try {
		const stream = bedrock.stream(request.model, request.context, options);
		const events: unknown[] = [];
		let eventCount = 0;
		for await (const event of stream) {
			events.push(JSON.parse(JSON.stringify(event)));
			eventCount += 1;
			if (
				request.abort !== null &&
				typeof request.abort === "object" &&
				eventCount >= request.abort.afterEvents
			) {
				controller.abort();
			}
		}
		const message = await stream.result();
		return {
			id: request.id,
			ok: true,
			output: {
				requests: activeRequests.map((item) => ({
					method: item.method,
					protocol: item.protocol,
					hostname: item.hostname,
					path: item.path,
					query: item.query ?? {},
					headers: item.headers,
					body:
						(item.body as { toString(): string } | undefined)?.toString() ?? "",
				})),
				events,
				message,
				capturedPayload: capturedPayload ?? null,
				capturedResponse: capturedResponse ?? null,
			},
		};
	} finally {
		restoreEnv(originals);
	}
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function executeLine(
	line: string,
	bedrock: {
		stream: (
			model: unknown,
			context: unknown,
			options: unknown,
		) => AsyncIterable<unknown> & { result: () => Promise<unknown> };
	},
): void {
	let id: RequestId = null;
	let request: OracleRequest | undefined;
	try {
		request = JSON.parse(line) as OracleRequest;
		id = request.id;
		void execute(request, bedrock).then(
			(response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			},
			(error) => {
				const response: OracleError = {
					id,
					ok: false,
					error: normalizeError(error),
				};
				process.stdout.write(`${JSON.stringify(response)}\n`);
			},
		);
	} catch (error) {
		const response: OracleError = {
			id,
			ok: false,
			error: normalizeError(error),
		};
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(bedrock: {
	stream: (
		model: unknown,
		context: unknown,
		options: unknown,
	) => AsyncIterable<unknown> & { result: () => Promise<unknown> };
}): void {
	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(
				() =>
					new Promise<void>((resolve) => {
						executeLine(line, bedrock);
						resolve();
					}),
			);
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`bedrock oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
