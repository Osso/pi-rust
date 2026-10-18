import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ToolDefinition = {
	name: string;
	label: string;
	description: string;
	promptSnippet?: string;
	promptGuidelines?: string[];
	parameters: unknown;
	executionMode?: string;
	execute(
		toolCallId: string,
		params: Record<string, unknown>,
	): Promise<{ content: unknown[]; details?: unknown; terminate?: boolean }>;
};

type ResponseToolApi = {
	SUPERVISOR_RESPONSE_TOOL_NAME: string;
	createSupervisorResponseTool(): ToolDefinition;
};

type Request =
	| { id: RequestId; operation: "metadata" }
	| { id: RequestId; operation: "execute"; params: Record<string, unknown> };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: ResponseToolApi): Promise<Response> {
	try {
		const tool = api.createSupervisorResponseTool();
		if (request.operation === "metadata") {
			return {
				id: request.id,
				ok: true,
				output: {
					constantName: api.SUPERVISOR_RESPONSE_TOOL_NAME,
					name: tool.name,
					label: tool.label,
					description: tool.description,
					promptSnippet: tool.promptSnippet ?? null,
					promptGuidelines: tool.promptGuidelines ?? null,
					parameters: tool.parameters,
					executionMode: tool.executionMode ?? null,
				},
			};
		}
		const result = await tool.execute("call-1", request.params);
		return {
			id: request.id,
			ok: true,
			output: {
				content: result.content,
				details: result.details ?? null,
				terminate: result.terminate ?? null,
			},
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: ResponseToolApi): void {
	let id: RequestId = null;
	void (async () => {
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const response = await execute(request, api);
			process.stdout.write(`${JSON.stringify(response)}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	})();
}

function serveRequests(api: ResponseToolApi): void {
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
		throw new Error("supervisor response tool oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ResponseToolApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`supervisor response tool oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
