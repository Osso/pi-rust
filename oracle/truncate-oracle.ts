import { readFile } from "node:fs/promises";
import { StringDecoder } from "node:string_decoder";
import { createContext, runInContext } from "node:vm";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type TruncationOptions = {
	maxLines?: number;
	maxBytes?: number;
};

type TruncationCase = {
	content: string;
	options: TruncationOptions;
};

type TruncateApi = {
	formatSize(bytes: number): string;
	truncateHead(content: string, options?: TruncationOptions): unknown;
	truncateTail(content: string, options?: TruncationOptions): unknown;
	truncateLine(line: string, maxChars?: number): { text: string; wasTruncated: boolean };
};

type ShellOutputApi = {
	sanitizeBinaryOutput(value: string): string;
};

type FileErrorCode =
	| "aborted"
	| "not_found"
	| "permission_denied"
	| "not_directory"
	| "is_directory"
	| "invalid"
	| "not_supported"
	| "unknown";

type ExecutionErrorCode =
	| "aborted"
	| "timeout"
	| "shell_unavailable"
	| "spawn_error"
	| "callback_error"
	| "unknown";

type HarnessErrorCauseRequest =
	| { kind: "error"; message: string }
	| {
			kind: "file";
			code: FileErrorCode;
			message: string;
			path?: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			kind: "execution";
			code: ExecutionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		};

type HarnessErrorRequest =
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "file";
			code: FileErrorCode;
			message: string;
			path?: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "execution";
			code: ExecutionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		};

type HarnessErrorInstance = Error & {
	code: FileErrorCode | ExecutionErrorCode;
	path?: string;
};

type HarnessTypesApi = {
	FileError: new (code: FileErrorCode, message: string, path?: string, cause?: Error) => HarnessErrorInstance;
	ExecutionError: new (code: ExecutionErrorCode, message: string, cause?: Error) => HarnessErrorInstance;
};

type NormalizedHarnessError = {
	kind: "FileError" | "ExecutionError" | "Error";
	name: string;
	message: string;
	code: FileErrorCode | ExecutionErrorCode | null;
	path: string | null;
	cause: NormalizedHarnessError | null;
};

type Request =
	| {
			id: RequestId;
			operation: "format_size";
			bytes: number;
		}
	| {
			id: RequestId;
			operation: "truncate_head" | "truncate_tail" | "truncate_head_without_buffer";
			content: string;
			options?: TruncationOptions;
		}
	| {
			id: RequestId;
			operation: "truncate_line";
			line: string;
			maxChars?: number;
		}
	| {
			id: RequestId;
			operation: "truncate_tail_batch";
			cases: TruncationCase[];
		}
	| {
			id: RequestId;
			operation: "sanitize_binary_output";
			text: string;
		}
	| {
			id: RequestId;
			operation: "sanitize_utf16_units";
			units: number[];
		}
	| HarnessErrorRequest;

type UtilityRequest = Exclude<Request, HarnessErrorRequest>;

type Response =
	| {
			id: RequestId;
			ok: true;
			output: unknown;
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

function toUtf16Units(value: string): number[] {
	return Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
}

function toCodePoints(value: string): number[] {
	return Array.from(value, (character) => character.codePointAt(0)).filter(
		(codePoint): codePoint is number => codePoint !== undefined,
	);
}

function normalizeSanitizedText(value: string): { text: string; utf16Units: number[]; codePoints: number[] } {
	return { text: value, utf16Units: toUtf16Units(value), codePoints: toCodePoints(value) };
}

function normalizeSanitizedUnits(value: string): { utf16Units: number[]; codePoints: number[] } {
	return { utf16Units: toUtf16Units(value), codePoints: toCodePoints(value) };
}

function successfulResponse(id: RequestId, output: unknown): Response {
	return { id, ok: true, output };
}

function normalizeTruncatedLine(
	api: TruncateApi,
	line: string,
	maxChars?: number,
): { textUtf16: number[]; wasTruncated: boolean } {
	const result = api.truncateLine(line, maxChars);
	return { textUtf16: toUtf16Units(result.text), wasTruncated: result.wasTruncated };
}

function normalizeTruncatedTailBatch(api: TruncateApi, cases: TruncationCase[]): { results: unknown[] } {
	return { results: cases.map(({ content, options }) => api.truncateTail(content, options)) };
}

function normalizeSanitizedUtf16Units(api: ShellOutputApi, units: number[]): { utf16Units: number[]; codePoints: number[] } {
	const value = String.fromCharCode(...units);
	return normalizeSanitizedUnits(api.sanitizeBinaryOutput(value));
}

function normalizeHarnessError(error: Error, api: HarnessTypesApi): NormalizedHarnessError {
	const harnessError = error as HarnessErrorInstance;
	const cause = normalizeHarnessCause(error, api);
	if (error instanceof api.FileError) {
		return normalizeFileHarnessError(error, harnessError, cause);
	}
	if (error instanceof api.ExecutionError) {
		return normalizeExecutionHarnessError(error, harnessError, cause);
	}
	return normalizeUnknownHarnessError(error, cause);
}

function normalizeHarnessCause(error: Error, api: HarnessTypesApi): NormalizedHarnessError | null {
	return error.cause instanceof Error ? normalizeHarnessError(error.cause, api) : null;
}

function normalizeFileHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "FileError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: harnessError.path ?? null,
		cause,
	};
}

function normalizeExecutionHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "ExecutionError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: null,
		cause,
	};
}

function normalizeUnknownHarnessError(error: Error, cause: NormalizedHarnessError | null): NormalizedHarnessError {
	return {
		kind: "Error",
		name: error.name,
		message: error.message,
		code: null,
		path: null,
		cause,
	};
}

function buildHarnessCause(request: HarnessErrorCauseRequest, api: HarnessTypesApi): Error {
	if (request.kind === "error") return new Error(request.message);
	const cause = request.cause === undefined ? undefined : buildHarnessCause(request.cause, api);
	if (request.kind === "file") return new api.FileError(request.code, request.message, request.path, cause);
	return new api.ExecutionError(request.code, request.message, cause);
}

function executeHarnessError(request: HarnessErrorRequest, api: HarnessTypesApi): Response {
	const cause = request.cause === undefined ? undefined : buildHarnessCause(request.cause, api);
	const error =
		request.kind === "file"
			? new api.FileError(request.code, request.message, request.path, cause)
			: new api.ExecutionError(request.code, request.message, cause);
	return successfulResponse(request.id, normalizeHarnessError(error, api));
}

function executeUtilityRequest(
	request: UtilityRequest,
	normal: TruncateApi,
	fallback: TruncateApi,
	shellOutput: ShellOutputApi,
): Response {
	switch (request.operation) {
		case "format_size":
			return successfulResponse(request.id, { value: normal.formatSize(request.bytes) });
		case "truncate_head":
			return successfulResponse(request.id, normal.truncateHead(request.content, request.options));
		case "truncate_tail":
			return successfulResponse(request.id, normal.truncateTail(request.content, request.options));
		case "truncate_head_without_buffer":
			return successfulResponse(request.id, fallback.truncateHead(request.content, request.options));
		case "truncate_line":
			return successfulResponse(request.id, normalizeTruncatedLine(normal, request.line, request.maxChars));
		case "truncate_tail_batch":
			return successfulResponse(request.id, normalizeTruncatedTailBatch(normal, request.cases));
		case "sanitize_binary_output":
			return successfulResponse(
				request.id,
				normalizeSanitizedText(shellOutput.sanitizeBinaryOutput(request.text)),
			);
		case "sanitize_utf16_units":
			return successfulResponse(request.id, normalizeSanitizedUtf16Units(shellOutput, request.units));
	}
}

function execute(request: Request, apis: OracleApis): Response {
	try {
		if (request.operation === "harness_error") {
			return executeHarnessError(request, apis.harnessTypes);
		}
		return executeUtilityRequest(request, apis.normal, apis.fallback, apis.shellOutput);
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

type EsbuildApi = {
	transform(
		source: string,
		options: { loader: "ts"; format: "cjs"; target: "es2022" },
	): Promise<{ code: string }>;
};

type EsbuildModule = {
	transform?: EsbuildApi["transform"];
	default?: EsbuildApi;
};

type CommonJsModule = {
	exports: Record<string, unknown>;
};

function readModulePaths(): [string, string, string, string] {
	const truncateModulePath = process.argv[2];
	const shellOutputModulePath = process.argv[3];
	const agentModulePath = process.argv[4];
	const esbuildModulePath = process.argv[5];
	if (
		truncateModulePath === undefined ||
		shellOutputModulePath === undefined ||
		agentModulePath === undefined ||
		esbuildModulePath === undefined
	) {
		throw new Error("agent utility oracle requires truncate, shell-output, agent-package, and esbuild module paths");
	}
	return [truncateModulePath, shellOutputModulePath, agentModulePath, esbuildModulePath];
}

async function loadFallbackApi(sourceModulePath: string, esbuildModulePath: string): Promise<TruncateApi> {
	const source = await readFile(sourceModulePath, "utf8");
	const imported = (await import(pathToFileURL(esbuildModulePath).href)) as EsbuildModule;
	const transform = imported.transform ?? imported.default?.transform;
	if (transform === undefined) {
		throw new Error("esbuild transform export is unavailable");
	}
	const transformed = await transform(source, { loader: "ts", format: "cjs", target: "es2022" });
	const module: CommonJsModule = { exports: {} };
	const context = createContext({ module, exports: module.exports });
	runInContext(transformed.code, context, { filename: sourceModulePath });
	return module.exports as unknown as TruncateApi;
}

type OracleApis = {
	normal: TruncateApi;
	fallback: TruncateApi;
	shellOutput: ShellOutputApi;
	harnessTypes: HarnessTypesApi;
};

async function loadApis(): Promise<OracleApis> {
	const [truncateModulePath, shellOutputModulePath, agentModulePath, esbuildModulePath] = readModulePaths();
	const normal = (await import(pathToFileURL(truncateModulePath).href)) as TruncateApi;
	const fallback = await loadFallbackApi(truncateModulePath, esbuildModulePath);
	const shellOutput = (await import(pathToFileURL(shellOutputModulePath).href)) as ShellOutputApi;
	const harnessTypes = (await import(pathToFileURL(agentModulePath).href)) as HarnessTypesApi;
	return { normal, fallback, shellOutput, harnessTypes };
}

function executeRequestLine(line: string, apis: OracleApis): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, apis))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(apis: OracleApis): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), apis);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	serveRequests(await loadApis());
}

void main().catch((error: unknown) => {
	process.stderr.write(`truncation oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
