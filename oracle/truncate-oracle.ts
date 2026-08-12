import { readFile } from "node:fs/promises";
import { createInterface } from "node:readline";
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

function execute(request: Request, normal: TruncateApi, fallback: TruncateApi): Response {
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

function readModulePaths(): [string, string] {
	const sourceModulePath = process.argv[2];
	const esbuildModulePath = process.argv[3];
	if (sourceModulePath === undefined || esbuildModulePath === undefined) {
		throw new Error("truncation oracle requires source and esbuild module paths");
	}
	return [sourceModulePath, esbuildModulePath];
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

async function loadApis(): Promise<{ normal: TruncateApi; fallback: TruncateApi }> {
	const [sourceModulePath, esbuildModulePath] = readModulePaths();
	const normal = (await import(pathToFileURL(sourceModulePath).href)) as TruncateApi;
	const fallback = await loadFallbackApi(sourceModulePath, esbuildModulePath);
	return { normal, fallback };
}

function serveRequests(apis: { normal: TruncateApi; fallback: TruncateApi }): void {
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	input.on("line", (line) => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify(execute(request, apis.normal, apis.fallback))}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
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
