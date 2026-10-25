import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ExtensionFlag = {
	name: string;
	description?: string;
	type: "boolean" | "string";
	default?: boolean | string;
	extensionPath: string;
};

type ArgsApi = {
	parseArgs(args: string[]): Record<string, unknown>;
	printHelp(extensionFlags?: ExtensionFlag[]): void;
	isValidThinkingLevel(level: string): boolean;
};

type Request =
	| { id: RequestId; operation: "parse_args"; args: string[] }
	| { id: RequestId; operation: "print_help"; extensionFlags: ExtensionFlag[] }
	| { id: RequestId; operation: "is_valid_thinking_level"; levels: string[] };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

const ARG_FIELDS = [
	"provider",
	"model",
	"apiKey",
	"systemPrompt",
	"appendSystemPrompt",
	"thinking",
	"continue",
	"resume",
	"help",
	"version",
	"mode",
	"name",
	"noSession",
	"session",
	"sessionId",
	"fork",
	"sessionDir",
	"worktree",
	"models",
	"tools",
	"excludeTools",
	"permissionPromptTool",
	"approvalPreset",
	"noTools",
	"noBuiltinTools",
	"extensions",
	"noExtensions",
	"print",
	"export",
	"exportTerminal",
	"exportColumns",
	"noSkills",
	"skills",
	"promptTemplates",
	"noPromptTemplates",
	"themes",
	"noThemes",
	"noContextFiles",
	"listModels",
	"listTools",
	"listExtensions",
	"loginProvider",
	"offline",
	"tuiMode",
	"verbose",
	"projectTrustOverride",
	"messages",
	"fileArgs",
] as const;

function normalizeArgs(parsed: Record<string, unknown>): Record<string, unknown> {
	const output: Record<string, unknown> = {};
	for (const field of ARG_FIELDS) {
		if (parsed[field] !== undefined) output[field] = parsed[field];
	}
	const unknownFlags = parsed.unknownFlags as Map<string, boolean | string>;
	output.unknownFlags = Array.from(unknownFlags.entries());
	output.diagnostics = parsed.diagnostics;
	return output;
}

function captureStdout(action: () => void): string {
	const original = process.stdout.write.bind(process.stdout);
	let captured = "";
	process.stdout.write = ((chunk: string | Uint8Array) => {
		captured += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
		return true;
	}) as typeof process.stdout.write;
	try {
		action();
	} finally {
		process.stdout.write = original;
	}
	return captured;
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: ArgsApi): Response {
	try {
		switch (request.operation) {
			case "parse_args":
				return { id: request.id, ok: true, output: normalizeArgs(api.parseArgs(request.args)) };
			case "print_help":
				return {
					id: request.id,
					ok: true,
					output: captureStdout(() => api.printHelp(request.extensionFlags)),
				};
			case "is_valid_thinking_level":
				return {
					id: request.id,
					ok: true,
					output: request.levels.map((level) => api.isValidThinkingLevel(level)),
				};
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: ArgsApi): void {
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

function serveRequests(api: ArgsApi): void {
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
		throw new Error("args oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ArgsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`args oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
