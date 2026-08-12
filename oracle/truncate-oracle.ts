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
		};

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

function execute(request: Request, normal: TruncateApi, fallback: TruncateApi, shellOutput: ShellOutputApi): Response {
	try {
		switch (request.operation) {
			case "format_size":
				return { id: request.id, ok: true, output: { value: normal.formatSize(request.bytes) } };
			case "truncate_head":
				return { id: request.id, ok: true, output: normal.truncateHead(request.content, request.options) };
			case "truncate_tail":
				return { id: request.id, ok: true, output: normal.truncateTail(request.content, request.options) };
			case "truncate_head_without_buffer":
				return { id: request.id, ok: true, output: fallback.truncateHead(request.content, request.options) };
			case "truncate_line": {
				const result = normal.truncateLine(request.line, request.maxChars);
				return {
					id: request.id,
					ok: true,
					output: { textUtf16: toUtf16Units(result.text), wasTruncated: result.wasTruncated },
				};
			}
			case "truncate_tail_batch":
				return {
					id: request.id,
					ok: true,
					output: {
						results: request.cases.map(({ content, options }) => normal.truncateTail(content, options)),
					},
				};
			case "sanitize_binary_output":
				return {
					id: request.id,
					ok: true,
					output: normalizeSanitizedText(shellOutput.sanitizeBinaryOutput(request.text)),
				};
			case "sanitize_utf16_units": {
				const value = String.fromCharCode(...request.units);
				return {
					id: request.id,
					ok: true,
					output: normalizeSanitizedUnits(shellOutput.sanitizeBinaryOutput(value)),
				};
			}
		}
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

function readModulePaths(): [string, string, string] {
	const truncateModulePath = process.argv[2];
	const shellOutputModulePath = process.argv[3];
	const esbuildModulePath = process.argv[4];
	if (truncateModulePath === undefined || shellOutputModulePath === undefined || esbuildModulePath === undefined) {
		throw new Error("agent utility oracle requires truncate, shell-output, and esbuild module paths");
	}
	return [truncateModulePath, shellOutputModulePath, esbuildModulePath];
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

async function loadApis(): Promise<{ normal: TruncateApi; fallback: TruncateApi; shellOutput: ShellOutputApi }> {
	const [truncateModulePath, shellOutputModulePath, esbuildModulePath] = readModulePaths();
	const normal = (await import(pathToFileURL(truncateModulePath).href)) as TruncateApi;
	const fallback = await loadFallbackApi(truncateModulePath, esbuildModulePath);
	const shellOutput = (await import(pathToFileURL(shellOutputModulePath).href)) as ShellOutputApi;
	return { normal, fallback, shellOutput };
}

function executeRequestLine(
	line: string,
	apis: { normal: TruncateApi; fallback: TruncateApi; shellOutput: ShellOutputApi },
): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, apis.normal, apis.fallback, apis.shellOutput))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(apis: { normal: TruncateApi; fallback: TruncateApi; shellOutput: ShellOutputApi }): void {
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
