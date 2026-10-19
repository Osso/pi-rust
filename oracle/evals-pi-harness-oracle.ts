import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ModelSelection = { provider: string; id: string };

type PiHarnessApi = {
	resolveModelSelection(
		explicitModel: ModelSelection | undefined,
		environment: { PI_PROVIDER?: string; PI_MODEL?: string },
	): ModelSelection;
};

type PiHarnessRequest = {
	id: RequestId;
	operation: "resolve_model_selection";
	explicitModel: ModelSelection | null;
	environment: { PI_PROVIDER?: string; PI_MODEL?: string };
};

type Response =
	| { id: RequestId; ok: true; output: { provider: string; id: string } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: PiHarnessRequest, api: PiHarnessApi): Response {
	try {
		const selection = api.resolveModelSelection(request.explicitModel ?? undefined, request.environment);
		return { id: request.id, ok: true, output: { provider: selection.provider, id: selection.id } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: PiHarnessApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as PiHarnessRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: PiHarnessApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("evals pi-harness oracle requires a source module path");
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as PiHarnessApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`evals pi-harness oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
