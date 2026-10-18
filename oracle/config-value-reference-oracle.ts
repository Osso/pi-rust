import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ConfigValueReferenceApi = {
	resolveConfigValue(config: string, env?: Record<string, string>): string | undefined;
	resolveConfigValueUncached(config: string, env?: Record<string, string>): string | undefined;
	resolveConfigValueOrThrow(config: string, description: string, env?: Record<string, string>): string;
	resolveHeaders(
		headers: Record<string, string> | undefined,
		env?: Record<string, string>,
	): Record<string, string> | undefined;
	resolveHeadersOrThrow(
		headers: Record<string, string> | undefined,
		description: string,
		env?: Record<string, string>,
	): Record<string, string> | undefined;
	clearConfigValueCache(): void;
	getConfigValueEnvVarName(config: string): string | undefined;
	getConfigValueEnvVarNames(config: string): string[];
	getMissingConfigValueEnvVarNames(config: string, env?: Record<string, string>): string[];
	isCommandConfigValue(config: string): boolean;
	isConfigValueConfigured(config: string, env?: Record<string, string>): boolean;
};

type ReferenceRequest = {
	id: RequestId;
	config: string;
};

type StatusRequest = {
	id: RequestId;
	operation: "status";
	config: string;
	env: Record<string, string> | null;
	processEnv?: Record<string, string | null>;
};

type ResolveRequest = {
	id: RequestId;
	operation: "resolve";
	config: string;
	env: Record<string, string> | null;
	processEnv?: Record<string, string | null>;
	uncached?: boolean;
};

type ThrowRequest = {
	id: RequestId;
	operation: "throw";
	config: string;
	description: string;
	env: Record<string, string> | null;
	processEnv?: Record<string, string | null>;
};

type HeadersRequest = {
	id: RequestId;
	operation: "headers";
	headers: Record<string, string> | null;
	description?: string;
	env: Record<string, string> | null;
	processEnv?: Record<string, string | null>;
	throwOnError?: boolean;
};

type Request = ReferenceRequest | StatusRequest | ResolveRequest | ThrowRequest | HeadersRequest;

type ReferenceOutput = {
	envVarName: string | null;
	envVarNames: string[];
	isCommand: boolean;
};

type StatusOutput = {
	missing: string[];
	configured: boolean;
};

type Output = ReferenceOutput | StatusOutput;

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

function withProcessEnvironment<T>(values: Record<string, string | null> | undefined, action: () => T): T {
	if (values === undefined) return action();

	const previous = new Map<string, string | undefined>();
	for (const [name, value] of Object.entries(values)) {
		previous.set(name, process.env[name]);
		if (value === null) delete process.env[name];
		else process.env[name] = value;
	}

	try {
		return action();
	} finally {
		for (const [name, value] of previous) {
			if (value === undefined) delete process.env[name];
			else process.env[name] = value;
		}
	}
}

function referenceOutput(config: string, api: ConfigValueReferenceApi): ReferenceOutput {
	return {
		envVarName: api.getConfigValueEnvVarName(config) ?? null,
		envVarNames: api.getConfigValueEnvVarNames(config),
		isCommand: api.isCommandConfigValue(config),
	};
}

function statusOutput(request: StatusRequest, api: ConfigValueReferenceApi): StatusOutput {
	const env = request.env ?? undefined;
	return withProcessEnvironment(request.processEnv, () => ({
		missing: api.getMissingConfigValueEnvVarNames(request.config, env),
		configured: api.isConfigValueConfigured(request.config, env),
	}));
}

function resolveOutput(request: ResolveRequest, api: ConfigValueReferenceApi): { value: string | null } {
	const env = request.env ?? undefined;
	return withProcessEnvironment(request.processEnv, () => {
		const value = request.uncached
			? api.resolveConfigValueUncached(request.config, env)
			: api.resolveConfigValue(request.config, env);
		return { value: value ?? null };
	});
}

function throwOutput(request: ThrowRequest, api: ConfigValueReferenceApi): { value: string } {
	const env = request.env ?? undefined;
	return withProcessEnvironment(request.processEnv, () => ({
		value: api.resolveConfigValueOrThrow(request.config, request.description, env),
	}));
}

function headersOutput(
	request: HeadersRequest,
	api: ConfigValueReferenceApi,
): { headers: Record<string, string> | null } {
	const env = request.env ?? undefined;
	const headers = request.headers ?? undefined;
	return withProcessEnvironment(request.processEnv, () => {
		const resolved = request.throwOnError
			? api.resolveHeadersOrThrow(headers, request.description ?? "Provider", env)
			: api.resolveHeaders(headers, env);
		return { headers: resolved ?? null };
	});
}

function execute(request: Request, api: ConfigValueReferenceApi): Response {
	try {
		if (!("operation" in request)) {
			return { id: request.id, ok: true, output: referenceOutput(request.config, api) };
		}
		const output =
			request.operation === "resolve"
				? resolveOutput(request, api)
				: request.operation === "throw"
					? throwOutput(request, api)
					: request.operation === "headers"
						? headersOutput(request, api)
						: statusOutput(request, api);
		return { id: request.id, ok: true, output };
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
