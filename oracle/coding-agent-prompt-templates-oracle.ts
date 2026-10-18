import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type PromptTemplate = {
	name: string;
	description: string;
	argumentHint?: string;
	content: string;
	sourceInfo: Record<string, unknown>;
	filePath: string;
};

type PromptTemplatesApi = {
	parseCommandArgs(argsString: string): string[];
	substituteArgs(content: string, args: string[]): string;
	expandPromptTemplate(text: string, templates: PromptTemplate[]): string;
	loadPromptTemplates(options: {
		cwd: string;
		agentDir: string;
		promptPaths: string[];
		includeDefaults: boolean;
	}): PromptTemplate[];
};

type Request =
	| { id: RequestId; operation: "parse_args"; input: string }
	| { id: RequestId; operation: "substitute"; content: string; args: string[] }
	| { id: RequestId; operation: "expand"; text: string; templates: PromptTemplate[] }
	| {
			id: RequestId;
			operation: "load";
			cwd: string;
			agentDir: string;
			promptPaths: string[];
			includeDefaults: boolean;
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

function execute(request: Request, api: PromptTemplatesApi): unknown {
	switch (request.operation) {
		case "parse_args":
			return { args: api.parseCommandArgs(request.input) };
		case "substitute":
			return { content: api.substituteArgs(request.content, request.args) };
		case "expand":
			return { content: api.expandPromptTemplate(request.text, request.templates) };
		case "load": {
			const templates = api.loadPromptTemplates({
				cwd: request.cwd,
				agentDir: request.agentDir,
				promptPaths: request.promptPaths,
				includeDefaults: request.includeDefaults,
			});
			templates.sort((left, right) =>
				left.filePath < right.filePath ? -1 : left.filePath > right.filePath ? 1 : 0,
			);
			return { templates };
		}
	}
}

function executeRequestLine(line: string, api: PromptTemplatesApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const response: Response = { id, ok: true, output: execute(request, api) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: PromptTemplatesApi): void {
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
		throw new Error("prompt templates oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as PromptTemplatesApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`prompt templates oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
