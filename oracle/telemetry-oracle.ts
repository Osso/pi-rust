import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type SpanOptions = {
	name: string;
	attributes?: Record<string, unknown>;
};

type TelemetrySpan = {
	startSpan<T>(options: SpanOptions, callback: (span: TelemetrySpan) => T | Promise<T>): Promise<T>;
	addEvent(name: string, attributes?: Record<string, unknown>): void;
	setAttributes(attributes: Record<string, unknown>): void;
	setStatus(status: unknown): void;
};

type TelemetryContext = {
	startSpan<T>(options: SpanOptions, callback: (span: TelemetrySpan) => T | Promise<T>): Promise<T>;
	getSpans(): readonly unknown[];
};

type TelemetryApi = {
	InMemoryTelemetryContext: new () => TelemetryContext;
	defineTelemetrySchema<T>(schema: T): T;
};

type TelemetryRequest = {
	id: RequestId;
	scenario: "recording" | "status" | "parentage" | "settled" | "schema";
};

type Response =
	| { id: RequestId; ok: true; output: Record<string, unknown> }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function sampleSchema() {
	return {
		version: 1,
		spans: {
			operation: {
				description: "Test operation",
				parents: { kind: "any" },
				startAttributes: {
					kind: { type: "string", required: true, values: ["read", "write"], description: "Kind" },
				},
				endAttributes: {},
				events: {
					result: {
						description: "Result",
						attributes: {
							outcome: { type: "string", required: true, values: ["ok", "error"], description: "Outcome" },
						},
					},
				},
				status: { default: "ok", errorWhen: "The operation fails" },
			},
		},
	};
}

async function runScenario(api: TelemetryApi, scenario: TelemetryRequest["scenario"]): Promise<Record<string, unknown>> {
	if (scenario === "schema") {
		return { schema: api.defineTelemetrySchema(sampleSchema()) };
	}

	const context = new api.InMemoryTelemetryContext();
	switch (scenario) {
		case "recording":
			await context.startSpan(
				{ name: "recording", attributes: { start: "value", overwrite: "start", ignored: undefined } },
				(span) => {
					span.setAttributes({ count: 1, overwrite: "middle" });
					span.setAttributes({ count: undefined, overwrite: "end" });
					span.addEvent("first", { index: 1, ignored: undefined });
					span.addEvent("second", { index: 2 });
				},
			);
			break;
		case "status":
			await context.startSpan({ name: "last-status" }, (span) => {
				span.setStatus({ status: "error", error: { name: "Expected", message: "first" } });
				span.setStatus({ status: "ok" });
			});
			await context.startSpan({ name: "expected-failure" }, (span) => {
				span.setStatus({ status: "error", error: { name: "Expected", message: "returned failure" } });
				return { ok: false };
			});
			break;
		case "parentage":
			await context.startSpan({ name: "parent" }, async (parent) => {
				await parent.startSpan({ name: "first-child" }, () => undefined);
				await parent.startSpan({ name: "second-child" }, () => undefined);
			});
			break;
		case "settled": {
			let settledSpan: TelemetrySpan | undefined;
			await context.startSpan({ name: "settled", attributes: { value: "initial" } }, (span) => {
				settledSpan = span;
			});
			if (!settledSpan) throw new Error("Expected callback span");
			settledSpan.setAttributes({ value: "late" });
			settledSpan.addEvent("late", { value: true });
			settledSpan.setStatus({ status: "error" });
			await settledSpan.startSpan({ name: "late-child" }, () => 7);
			break;
		}
	}
	return { spans: context.getSpans() };
}

async function execute(request: TelemetryRequest, api: TelemetryApi): Promise<Response> {
	try {
		return { id: request.id, ok: true, output: await runScenario(api, request.scenario) };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: TelemetryApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as TelemetryRequest;
		id = request.id;
		void execute(request, api).then((response) => {
			process.stdout.write(`${JSON.stringify(response)}\n`);
		});
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: TelemetryApi): void {
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
		throw new Error("telemetry oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as TelemetryApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`telemetry oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
