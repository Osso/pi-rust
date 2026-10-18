import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

interface Request {
	id: number;
	operation: "convertMessages" | "convertTools" | "processStream";
	model?: Record<string, unknown>;
	context?: Record<string, unknown>;
	allowed?: string[];
	options?: Record<string, unknown>;
	tools?: Array<Record<string, unknown>>;
	events?: Array<Record<string, unknown>>;
	output?: Record<string, unknown>;
}

interface Response {
	id: number | null;
	ok: boolean;
	output?: unknown;
	error?: string;
}

type Module = {
	convertResponsesMessages: (
		model: unknown,
		context: unknown,
		allowed: ReadonlySet<string>,
		options?: unknown,
	) => unknown;
	convertResponsesTools: (tools: unknown, options?: unknown) => unknown;
	processResponsesStream: (
		events: AsyncIterable<unknown>,
		output: unknown,
		stream: { push: (event: unknown) => void },
		model: unknown,
		options?: unknown,
	) => Promise<void>;
};

function stripStreamingScratch(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(stripStreamingScratch);
	if (!value || typeof value !== "object") return value;
	const record = value as Record<string, unknown>;
	const result: Record<string, unknown> = {};
	for (const [key, entry] of Object.entries(record)) {
		if (key === "partialJson" || key === "customInput" || key === "streamIndex" || key === "partialArgs") continue;
		result[key] = stripStreamingScratch(entry);
	}
	return result;
}

function snapshotEvent(event: unknown): unknown {
	return stripStreamingScratch(JSON.parse(JSON.stringify(event)));
}

function toMap(value: unknown): Map<string, unknown> | undefined {
	if (!value || typeof value !== "object") return undefined;
	return new Map(Object.entries(value as Record<string, unknown>));
}

function normalizeOptions(options: Record<string, unknown> | undefined): Record<string, unknown> | undefined {
	if (!options) return undefined;
	const normalized: Record<string, unknown> = { ...options };
	if (options.grammarToolInputProperties !== undefined) {
		normalized.grammarToolInputProperties = toMap(options.grammarToolInputProperties);
	}
	if (options.deferredTools !== undefined) {
		normalized.deferredTools = toMap(options.deferredTools);
	}
	if (options.toolOptions !== undefined) {
		normalized.toolOptions = normalizeOptions(options.toolOptions as Record<string, unknown>);
	}
	return normalized;
}

async function* toEvents(events: Array<Record<string, unknown>>): AsyncIterable<unknown> {
	for (const event of events) yield event;
}

function normalizeError(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

async function execute(request: Request, module: Module): Promise<unknown> {
	switch (request.operation) {
		case "convertMessages":
			return module.convertResponsesMessages(
				request.model,
				request.context,
				new Set(request.allowed ?? []),
				normalizeOptions(request.options),
			);
		case "convertTools":
			return module.convertResponsesTools(request.tools, normalizeOptions(request.options));
		case "processStream": {
			const events: Array<unknown> = [];
			const stream = { push: (event: unknown) => events.push(snapshotEvent(event)) };
			await module.processResponsesStream(
				toEvents(request.events ?? []),
				request.output,
				stream,
				request.model,
				normalizeOptions(request.options),
			);
			return { events, output: request.output };
		}
	}
}

function executeRequestLine(line: string, module: Module): void {
	let id: number | null = null;
	let response: Response;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		execute(request, module)
			.then((output) => {
				process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
			})
			.catch((error: unknown) => {
				process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
			});
	} catch (error) {
		response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(module: Module): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), module);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("openai responses shared oracle requires a source module path");
	const module = (await import(pathToFileURL(modulePath).href)) as Module;
	serveRequests(module);
}

void main().catch((error: unknown) => {
	process.stderr.write(`openai responses shared oracle startup failed: ${normalizeError(error)}\n`);
	process.exitCode = 1;
});
