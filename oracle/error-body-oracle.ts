import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ErrorCase = {
	name: string;
	message: string;
	statusCode?: number;
	status?: number;
	body?: unknown;
	error?: unknown;
	metadataHttpStatusCode?: number;
	responseStatusCode?: number;
	responseBody?: unknown;
	prefix?: string;
};

type Request =
	| { id: RequestId; operation: "normalize" | "normalize_utf16"; cases: ErrorCase[] }
	| {
			id: RequestId;
			operation: "format";
			cases: Array<{
				status?: number | "NaN" | "Infinity" | "-Infinity";
				body?: string;
				message: string;
				messageCarriesBody: boolean;
				prefix?: string;
			}>;
	  }
	| { id: RequestId; operation: "normalize_value"; cases: Array<{ name: string; value: unknown; prefix?: string }> }
	| { id: RequestId; operation: "truncate"; cases: Array<{ name: string; text: string; maxChars: number }> };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function buildError(item: ErrorCase): Error {
	const error = Object.assign(new Error(item.message), {});
	if (item.statusCode !== undefined) Object.assign(error, { statusCode: item.statusCode });
	if (item.status !== undefined) Object.assign(error, { status: item.status });
	if (item.body !== undefined) Object.assign(error, { body: item.body });
	if (item.error !== undefined) Object.assign(error, { error: item.error });
	if (item.metadataHttpStatusCode !== undefined) {
		Object.assign(error, { $metadata: { httpStatusCode: item.metadataHttpStatusCode } });
	}
	if (item.responseStatusCode !== undefined || item.responseBody !== undefined) {
		Object.assign(error, {
			$response: { statusCode: item.responseStatusCode, body: item.responseBody },
		});
	}
	return error;
}

function serializeNormalized(norm: never, formatProviderError: (norm: never, prefix?: string) => string, prefix?: string) {
	return {
		status: (norm as { status?: number }).status ?? null,
		body: (norm as { body?: string }).body ?? null,
		message: (norm as { message: string }).message,
		messageCarriesBody: (norm as { messageCarriesBody: boolean }).messageCarriesBody,
		formatted: formatProviderError(norm, prefix ?? undefined),
		formattedWithoutPrefix: formatProviderError(norm),
	};
}

function utf16Units(text: string): number[] {
	return Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
}

function execute(request: Request, api: Record<string, unknown>): Response {
	const normalizeProviderError = api.normalizeProviderError as (error: unknown) => never;
	const formatProviderError = api.formatProviderError as (
		norm: never,
		prefix?: string,
	) => string;

	if (request.operation === "normalize") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => ({
					name: item.name,
					...serializeNormalized(normalizeProviderError(buildError(item)), formatProviderError, item.prefix),
				})),
			},
		};
	}
	if (request.operation === "normalize_utf16") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => {
					const norm = normalizeProviderError(buildError(item)) as { body?: string };
					return { bodyUnits: norm.body === undefined ? null : utf16Units(norm.body) };
				}),
			},
		};
	}
	if (request.operation === "format") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => {
					const status = typeof item.status === "string" ? Number(item.status) : item.status;
					const norm = { ...item, status };
					return formatProviderError(norm as never, item.prefix);
				}),
			},
		};
	}
	if (request.operation === "normalize_value") {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => ({
					name: item.name,
					...serializeNormalized(normalizeProviderError(item.value), formatProviderError, item.prefix),
				})),
			},
		};
	}
	if (request.operation === "truncate") {
		const truncateErrorText = api.truncateErrorText as (text: string, maxChars: number) => string;
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => ({
					name: item.name,
					units: utf16Units(truncateErrorText(item.text, item.maxChars)),
				})),
			},
		};
	}
	throw new Error(`unsupported operation: ${(request as { operation: string }).operation}`);
}

function executeRequestLine(line: string, api: Record<string, unknown>): void {
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

function serveRequests(api: Record<string, unknown>): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("error-body oracle requires a source module path");
	const api = (await import(pathToFileURL(`${modulePath}/utils/error-body.ts`).href)) as Record<string, unknown>;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`error-body oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
