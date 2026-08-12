import { Readable } from "node:stream";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type JsonlApi = {
	serializeJsonLine(value: unknown): string;
	attachJsonlLineReader(stream: Readable, onLine: (line: string) => void): () => void;
};

type Chunk =
	| {
			kind: "string";
			value: string;
		}
	| {
			kind: "bytes_hex";
			value: string;
		};

type Request =
	| {
			id: RequestId;
			operation: "serialize_json_line";
			value: unknown;
		}
	| {
			id: RequestId;
			operation: "read_jsonl_sequence";
			chunks: Chunk[];
		};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { serialized: string } | { lines: string[] };
		}
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
		};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function executeReadSequence(request: Extract<Request, { operation: "read_jsonl_sequence" }>, jsonl: JsonlApi): Response {
	const stream = new Readable({ read() {} });
	const lines: string[] = [];
	const detach = jsonl.attachJsonlLineReader(stream, (line) => lines.push(line));
	for (const chunk of request.chunks) {
		const value = chunk.kind === "string" ? chunk.value : Buffer.from(chunk.value, "hex");
		stream.emit("data", value);
	}
	stream.emit("end");
	detach();
	return { id: request.id, ok: true, output: { lines } };
}

function execute(request: Request, jsonl: JsonlApi): Response {
	try {
		switch (request.operation) {
			case "serialize_json_line":
				return {
					id: request.id,
					ok: true,
					output: { serialized: jsonl.serializeJsonLine(request.value) },
				};
			case "read_jsonl_sequence":
				return executeReadSequence(request, jsonl);
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function readModulePath(): string {
	const modulePath = process.argv[2];
	if (modulePath === undefined) {
		throw new Error("RPC JSONL oracle requires the source module path");
	}
	return modulePath;
}

function serveRequests(jsonl: JsonlApi): void {
	jsonl.attachJsonlLineReader(process.stdin, (line) => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify(execute(request, jsonl))}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = readModulePath();
	const jsonl = (await import(pathToFileURL(modulePath).href)) as JsonlApi;
	serveRequests(jsonl);
}

void main().catch((error: unknown) => {
	process.stderr.write(`RPC JSONL oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
