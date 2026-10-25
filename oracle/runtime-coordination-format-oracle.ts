import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type RuntimeMailboxMessage = {
	id: number;
	recipient: { agentId: string | null; sessionId: string };
	sender: { agentId: string | null; sessionId: string };
	senderHost?: string;
	kind: string;
	body: string;
	fileRefs?: Array<{ label?: string; path: string }>;
	storeRef: { messageId: string; sessionPath: string };
	status: string;
	createdAt: string;
	updatedAt: string;
};

type SharedChannelMessage = {
	originHost?: string;
	body: string;
	createdAt: string;
	id: number;
	sender: { agentId: string | null; sessionId: string };
};

type RuntimeCoordinationFormatApi = {
	formatRuntimeMailboxPrompt(message: RuntimeMailboxMessage, recipientSessionId: string): string;
	formatSharedChannelPrompt(messages: SharedChannelMessage[], recipientSessionId: string): string;
};

type Request = {
	id: RequestId;
	operation: "format";
	cases: Array<{
		name: string;
		recipientSessionId: string;
		message?: RuntimeMailboxMessage;
		messages?: SharedChannelMessage[];
	}>;
};

type Response =
	| { id: RequestId; ok: true; output: { results: Array<{ name: string; prompt: string }> } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: RuntimeCoordinationFormatApi): Response {
	try {
		const results = request.cases.map((testCase) => ({
			name: testCase.name,
			prompt:
				testCase.messages === undefined
					? api.formatRuntimeMailboxPrompt(testCase.message as RuntimeMailboxMessage, testCase.recipientSessionId)
					: api.formatSharedChannelPrompt(testCase.messages, testCase.recipientSessionId),
		}));
		return { id: request.id, ok: true, output: { results } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: RuntimeCoordinationFormatApi): void {
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

function serveRequests(api: RuntimeCoordinationFormatApi): void {
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
		throw new Error("runtime coordination format oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as RuntimeCoordinationFormatApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`runtime coordination format oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
