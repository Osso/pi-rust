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

type SchemaApi = {
	ModelCostSchema: unknown;
};

type TypeboxApi = {
	Check(schema: unknown, value: unknown): boolean;
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

type TaggedNumber =
	| {
			number: "finite";
			value: number;
		}
	| {
			number: "negative_zero" | "nan" | "positive_infinity" | "negative_infinity";
		};

type ModelCostRequest = {
		input: TaggedNumber;
		output: TaggedNumber;
		cacheRead: TaggedNumber;
		cacheWrite: TaggedNumber;
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
		}
	| {
			id: RequestId;
			operation: "validate_model_cost";
			value: ModelCostRequest;
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
			output: { frame_hex: string } | null | { results: DecoderStepResponse[] } | ModelCostResponse;
		}
	| {
			id: RequestId;
			ok: false;
			error: {
				name: string;
				message: string;
			};
		};

type ModelCostResponse =
	| {
			valid: false;
		}
	| {
			valid: true;
			bits: {
				input: string;
				output: string;
				cacheRead: string;
				cacheWrite: string;
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

function decodeTaggedNumber(number: TaggedNumber): number {
	switch (number.number) {
		case "finite":
			return number.value;
		case "negative_zero":
			return -0;
		case "nan":
			return Number.NaN;
		case "positive_infinity":
			return Number.POSITIVE_INFINITY;
		case "negative_infinity":
			return Number.NEGATIVE_INFINITY;
	}
}

function float64Bits(value: number): string {
	const buffer = new ArrayBuffer(8);
	const view = new DataView(buffer);
	view.setFloat64(0, value, false);
	return view.getBigUint64(0, false).toString(16).padStart(16, "0");
}

function executeModelCost(
	request: Extract<Request, { operation: "validate_model_cost" }>,
	schema: SchemaApi,
	typebox: TypeboxApi,
): Response {
	const value = {
		input: decodeTaggedNumber(request.value.input),
		output: decodeTaggedNumber(request.value.output),
		cacheRead: decodeTaggedNumber(request.value.cacheRead),
		cacheWrite: decodeTaggedNumber(request.value.cacheWrite),
	};
	if (!typebox.Check(schema.ModelCostSchema, value)) {
		return { id: request.id, ok: true, output: { valid: false } };
	}
	return {
		id: request.id,
		ok: true,
		output: {
			valid: true,
			bits: {
				input: float64Bits(value.input),
				output: float64Bits(value.output),
				cacheRead: float64Bits(value.cacheRead),
				cacheWrite: float64Bits(value.cacheWrite),
			},
		},
	};
}

function execute(request: Request, framing: FramingApi, schema: SchemaApi, typebox: TypeboxApi): Response {
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
			case "validate_model_cost":
				return executeModelCost(request, schema, typebox);
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function readModulePaths(): [string, string, string] {
	const framingModulePath = process.argv[2];
	const schemaModulePath = process.argv[3];
	const typeboxModulePath = process.argv[4];
	if (framingModulePath === undefined || schemaModulePath === undefined || typeboxModulePath === undefined) {
		throw new Error("protocol oracle requires framing, schema, and typebox module paths");
	}
	return [framingModulePath, schemaModulePath, typeboxModulePath];
}

async function loadApis(): Promise<{ framing: FramingApi; schema: SchemaApi; typebox: TypeboxApi }> {
	const [framingModulePath, schemaModulePath, typeboxModulePath] = readModulePaths();
	const framing = (await import(pathToFileURL(framingModulePath).href)) as FramingApi;
	const schema = (await import(pathToFileURL(schemaModulePath).href)) as SchemaApi;
	const typebox = (await import(pathToFileURL(typeboxModulePath).href)) as TypeboxApi;
	return { framing, schema, typebox };
}

function serveRequests(apis: { framing: FramingApi; schema: SchemaApi; typebox: TypeboxApi }): void {
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	input.on("line", (line) => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify(execute(request, apis.framing, apis.schema, apis.typebox))}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	});
}

async function main(): Promise<void> {
	const apis = await loadApis();
	serveRequests(apis);
}

void main().catch((error: unknown) => {
	process.stderr.write(`protocol oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
