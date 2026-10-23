import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Skill = {
	name: string;
	description: string;
	filePath: string;
	baseDir: string;
	sourceInfo: Record<string, unknown>;
	disableModelInvocation: boolean;
};

type BuildOptions = {
	customPrompt?: string;
	selectedTools?: string[];
	toolSnippets?: Record<string, string>;
	promptGuidelines?: string[];
	appendSystemPrompt?: string;
	cwd: string;
	contextFiles?: Array<{ path: string; content: string }>;
	rulesContent?: string;
	skills?: Skill[];
};

type SystemPromptApi = {
	buildSystemPrompt(options: BuildOptions): string;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function buildWithClock(api: SystemPromptApi, options: BuildOptions, nowMillis: number, timeZone: string): string {
	const RealDate = globalThis.Date;
	process.env.TZ = timeZone;
	globalThis.Date = class extends RealDate {
		constructor() { super(nowMillis); }
		static now() { return nowMillis; }
	} as DateConstructor;
	try {
		return api.buildSystemPrompt(options);
	} finally {
		globalThis.Date = RealDate;
	}
}

function executeRequestLine(line: string, api: SystemPromptApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as { id: RequestId; options: BuildOptions; nowMillis: number; timeZone: string };
		id = request.id;
		const response: Response = { id, ok: true, output: { prompt: buildWithClock(api, request.options, request.nowMillis, request.timeZone) } };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SystemPromptApi): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("system prompt oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as SystemPromptApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`system prompt oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
