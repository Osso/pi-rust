import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = {
	id: number;
	messages: unknown[];
	model: unknown;
	normalize: "anthropic" | "none";
	now: number;
};

type AssistantMessage = { provider?: string; api?: string; model?: string };
type Message = { role?: string; isError?: boolean; content?: Array<{ type?: string; text?: string }>; timestamp?: number };

type TransformApi = {
	transformMessages(
		messages: unknown[],
		model: unknown,
		normalizeToolCallId?: (id: string, model: unknown, source: AssistantMessage) => string,
	): Message[];
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function anthropicNormalizeToolCallId(id: string): string {
	return id.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 64);
}

// The TypeScript module stamps synthetic results with Date.now(); the oracle replaces
// those timestamps with the request clock so the comparison is deterministic.
function normalizeSyntheticTimestamps(messages: Message[], now: number): Message[] {
	return messages.map((message) => {
		const content = message.content;
		const isSynthetic =
			message.role === "toolResult" &&
			message.isError === true &&
			Array.isArray(content) &&
			content.length === 1 &&
			content[0]?.type === "text" &&
			content[0]?.text === "No result provided";
		return isSynthetic ? { ...message, timestamp: now } : message;
	});
}

function execute(request: Request, api: TransformApi) {
	try {
		const normalize = request.normalize === "anthropic" ? anthropicNormalizeToolCallId : undefined;
		const messages = api.transformMessages(request.messages, request.model, normalize);
		return {
			id: request.id,
			ok: true,
			output: { messages: normalizeSyntheticTimestamps(messages, request.now) },
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: TransformApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			process.stdout.write(`${JSON.stringify(execute(JSON.parse(buffer.slice(0, lineFeed)), api))}\n`);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("transform messages oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as TransformApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`transform messages oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
