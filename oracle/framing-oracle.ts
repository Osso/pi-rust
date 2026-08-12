import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type FrameDecoderOptions = {
	maxFrameLength?: number;
};

type FrameDecoderInstance = {
	push(chunk: Uint8Array): Uint8Array[];
	end(): void;
};

type FramingApi = {
	encodeFrame(payload: Uint8Array): Uint8Array;
	assertCompleteFrame(frame: Uint8Array, options?: FrameDecoderOptions): void;
	FrameDecoder: new (options?: FrameDecoderOptions) => FrameDecoderInstance;
};

type DecoderStep =
	| {
			op: "push";
			chunk_hex: string;
			mutate_fill?: number;
		}
	| {
			op: "end";
		};

type Request =
	| {
			id: RequestId;
			operation: "encode_frame";
			payload_hex: string;
		}
	| {
			id: RequestId;
			operation: "assert_complete_frame";
			frame_hex: string;
			max_frame_length?: number;
		}
	| {
			id: RequestId;
			operation: "decoder_sequence";
			max_frame_length?: number;
			steps: DecoderStep[];
		};

type DecoderStepResponse =
	| {
			ok: true;
			frames_hex: string[];
		}
	| {
			ok: true;
		}
	| {
			ok: false;
			error: {
				name: string;
				message: string;
			};
		};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { frame_hex: string } | null | { results: DecoderStepResponse[] };
		}
	| {
			id: RequestId;
			ok: false;
			error: {
				name: string;
				message: string;
			};
		};

function decodeHex(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes;
}

function encodeHex(bytes: Uint8Array): string {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function executeDecoderStep(decoder: FrameDecoderInstance, step: DecoderStep): DecoderStepResponse {
	try {
		switch (step.op) {
			case "push": {
				const chunk = decodeHex(step.chunk_hex);
				const frames = decoder.push(chunk);
				if (step.mutate_fill !== undefined) chunk.fill(step.mutate_fill);
				return { ok: true, frames_hex: frames.map(encodeHex) };
			}
			case "end":
				decoder.end();
				return { ok: true };
		}
	} catch (error) {
		return { ok: false, error: normalizeError(error) };
	}
}

function executeDecoderSequence(
	request: Extract<Request, { operation: "decoder_sequence" }>,
	framing: FramingApi,
): Response {
	const options = request.max_frame_length === undefined ? undefined : { maxFrameLength: request.max_frame_length };
	const decoder = new framing.FrameDecoder(options);
	const results = request.steps.map((step) => executeDecoderStep(decoder, step));
	return { id: request.id, ok: true, output: { results } };
}

function execute(request: Request, framing: FramingApi): Response {
	try {
		switch (request.operation) {
			case "encode_frame":
				return {
					id: request.id,
					ok: true,
					output: { frame_hex: encodeHex(framing.encodeFrame(decodeHex(request.payload_hex))) },
				};
			case "assert_complete_frame":
				framing.assertCompleteFrame(
					decodeHex(request.frame_hex),
					request.max_frame_length === undefined ? undefined : { maxFrameLength: request.max_frame_length },
				);
				return { id: request.id, ok: true, output: null };
			case "decoder_sequence":
				return executeDecoderSequence(request, framing);
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function readFramingModulePath(): string {
	const framingModulePath = process.argv[2];
	if (framingModulePath === undefined) {
		throw new Error("framing oracle requires the pinned TypeScript module path");
	}
	return framingModulePath;
}

async function loadFramingApi(): Promise<FramingApi> {
	const framingModuleUrl = pathToFileURL(readFramingModulePath()).href;
	return (await import(framingModuleUrl)) as FramingApi;
}

function serveRequests(framing: FramingApi): void {
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	input.on("line", (line) => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify(execute(request, framing))}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	});
}

async function main(): Promise<void> {
	const framing = await loadFramingApi();
	serveRequests(framing);
}

void main().catch((error: unknown) => {
	process.stderr.write(`framing oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
