import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ConfigValueReferenceApi = {
	getConfigValueEnvVarName(config: string): string | undefined;
	getConfigValueEnvVarNames(config: string): string[];
	isCommandConfigValue(config: string): boolean;
};

type Request = {
	id: RequestId;
	config: string;
};

type Output = {
	envVarName: string | null;
	envVarNames: string[];
	isCommand: boolean;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: Output;
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

function execute(request: Request, api: ConfigValueReferenceApi): Response {
	try {
		return {
			id: request.id,
			ok: true,
			output: {
				envVarName: api.getConfigValueEnvVarName(request.config) ?? null,
				envVarNames: api.getConfigValueEnvVarNames(request.config),
				isCommand: api.isCommandConfigValue(request.config),
			},
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: ConfigValueReferenceApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: ConfigValueReferenceApi): void {
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
	if (sourceModulePath === undefined) {
		throw new Error("config-value reference oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ConfigValueReferenceApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`config-value reference oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
