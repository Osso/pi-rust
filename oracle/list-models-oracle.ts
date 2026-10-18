import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ModelWire = Record<string, unknown>;

type ListModelsApi = {
	listModels(
		runtime: {
			getError(): string | undefined;
			getAvailable(): Promise<readonly ModelWire[]>;
		},
		searchPattern?: string,
		signal?: AbortSignal,
	): Promise<void>;
};

type Request = {
	id: RequestId;
	operation: "list_models";
	models: ModelWire[];
	searchPattern?: string;
	loadError?: string;
};

type Response =
	| { id: RequestId; ok: true; output: { warning: string | null; stdout: string } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function captureStreams(action: () => Promise<void>): Promise<{ warning: string | null; stdout: string }> {
	return (async () => {
		const originalStdout = process.stdout.write.bind(process.stdout);
		const originalStderr = process.stderr.write.bind(process.stderr);
		let stdout = "";
		let stderr = "";
		process.stdout.write = ((chunk: string | Uint8Array) => {
			stdout += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
			return true;
		}) as typeof process.stdout.write;
		process.stderr.write = ((chunk: string | Uint8Array) => {
			stderr += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
			return true;
		}) as typeof process.stderr.write;
		try {
			await action();
		} finally {
			process.stdout.write = originalStdout;
			process.stderr.write = originalStderr;
		}
		return { warning: stderr.length > 0 ? stderr.replace(/\n$/, "") : null, stdout };
	})();
}

function execute(request: Request, api: ListModelsApi): Promise<Response> {
	return (async () => {
		try {
			const runtime = {
				getError: () => request.loadError,
				getAvailable: async () => request.models,
			};
			const output = await captureStreams(() => api.listModels(runtime, request.searchPattern));
			return { id: request.id, ok: true, output };
		} catch (error) {
			return { id: request.id, ok: false, error: normalizeError(error) };
		}
	})();
}

function executeRequestLine(line: string, api: ListModelsApi): Promise<void> {
	return (async () => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const response = await execute(request, api);
			process.stdout.write(`${JSON.stringify(response)}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	})();
}

function serveRequests(api: ListModelsApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(() => executeRequestLine(line, api));
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("list-models oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ListModelsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`list-models oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
